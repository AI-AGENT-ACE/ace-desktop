//! Used by build.rs and at runtime. Metadata is an attestation, not an accuracy test.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, fs, path::Path};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    format_version: u32,
    model_version: String,
    engine: String,
    file: String,
    size_bytes: u64,
    sha256: String,
    redistribution_approved: bool,
    consent_record: String,
    evaluation_record: String,
    #[serde(default)]
    release_policy: ReleasePolicy,
}
#[derive(Deserialize, Default, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum ReleasePolicy {
    #[default]
    EvaluatedPublic,
    PersonalPreview,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Evaluation {
    model_sha256: String,
    held_out_speakers: Vec<Speaker>,
    negative_audio_seconds: u64,
    false_activations: u64,
    #[serde(default)]
    status: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Speaker {
    participant_id: String,
    attempts: u64,
    detections: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Consent {
    redistribution_approved: bool,
    scope: String,
    record_reference: String,
}
#[derive(Deserialize)]
struct Reference {
    name: String,
    mfcc_size: u16,
    rms_level: f32,
    threshold: Option<f32>,
    avg_threshold: Option<f32>,
    samples_features: HashMap<String, Vec<Vec<f32>>>,
    avg_features: Option<Vec<Vec<f32>>>,
}
fn child(root: &Path, name: &str) -> Result<std::path::PathBuf, String> {
    if name.is_empty()
        || Path::new(name)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("manifest paths must be relative normal paths".into());
    }
    let target = root
        .join(name)
        .canonicalize()
        .map_err(|_| format!("missing or unreadable resource: {name}"))?;
    if !target.starts_with(root.canonicalize().map_err(|e| e.to_string())?) {
        return Err("resource path escapes model directory".into());
    }
    Ok(target)
}
pub fn validate(root: &Path) -> Result<std::path::PathBuf, String> {
    let manifest: Manifest = serde_json::from_slice(&fs::read(root.join("manifest.json")).map_err(|_| "ACE_DEFAULT_MODEL_MISSING: resources/wake-word/manifest.json and an approved public ACE model are required")?).map_err(|e| format!("invalid model manifest: {e}"))?;
    let version: Vec<_> = manifest.model_version.split('.').collect();
    if manifest.format_version != 1
        || manifest.engine != "rustpotter-3.0.2-ref"
        || version.len() != 3
        || version.iter().any(|v| v.parse::<u32>().is_err())
    {
        return Err("unsupported model format, engine or version".into());
    }
    if !manifest.redistribution_approved {
        return Err("public model redistribution approval is required".into());
    }
    if manifest.file != "ace.rpw" {
        return Err(
            "public model filename must be ace.rpw (personal models are not release resources)"
                .into(),
        );
    }
    let model = child(root, &manifest.file)?;
    let size = fs::metadata(&model).map_err(|e| e.to_string())?.len();
    if !(1024..=16 * 1024 * 1024).contains(&size) || size != manifest.size_bytes {
        return Err("invalid model size (expected 1 KiB..16 MiB and manifest match)".into());
    }
    let bytes = fs::read(&model).map_err(|e| e.to_string())?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    if hash != manifest.sha256 {
        return Err("model SHA-256 does not match manifest".into());
    }
    let mut cursor = std::io::Cursor::new(&bytes);
    let reference: Reference = ciborium::from_reader(&mut cursor)
        .map_err(|e| format!("invalid Rustpotter CBOR reference: {e}"))?;
    let valid_frames = |frames: &Vec<Vec<f32>>| {
        !frames.is_empty()
            && frames.len() <= 2000
            && frames
                .iter()
                .all(|f| f.len() == reference.mfcc_size as usize && f.iter().all(|n| n.is_finite()))
    };
    if cursor.position() != bytes.len() as u64
        || !reference.name.eq_ignore_ascii_case("ACE")
        || reference.mfcc_size != 13
        || !reference.rms_level.is_finite()
        || reference.rms_level <= 0.0
        || reference.samples_features.is_empty()
        || reference
            .samples_features
            .values()
            .any(|v| !valid_frames(v))
        || reference
            .avg_features
            .as_ref()
            .is_some_and(|v| !valid_frames(v))
        || [reference.threshold, reference.avg_threshold]
            .iter()
            .flatten()
            .any(|n| !n.is_finite() || !(0.0..=1.0).contains(n))
    {
        return Err("invalid ACE model feature shape or numeric values".into());
    }
    let consent: Consent = serde_json::from_slice(
        &fs::read(child(root, &manifest.consent_record)?).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if !consent.redistribution_approved
        || consent.scope != "model-and-derived-features"
        || consent.record_reference.trim().is_empty()
    {
        return Err("model and derived feature redistribution consent is required".into());
    }
    let report: Evaluation = serde_json::from_slice(
        &fs::read(child(root, &manifest.evaluation_record)?).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut participants = std::collections::HashSet::new();
    // Explicit owner-authorized preview; never represent missing evaluation as a pass.
    if manifest.release_policy == ReleasePolicy::PersonalPreview {
        if report.model_sha256 != hash
            || report.status.as_deref() != Some("pending")
            || !report.held_out_speakers.is_empty()
            || report.negative_audio_seconds != 0
            || report.false_activations != 0
        {
            return Err("personal preview requires an explicit pending evaluation record".into());
        }
        return Ok(model);
    }
    if report.model_sha256 != hash
        || report.held_out_speakers.len() < 3
        || report.held_out_speakers.iter().any(|s| {
            s.participant_id.is_empty()
                || !participants.insert(&s.participant_id)
                || s.attempts < 10
                || s.detections > s.attempts
                || s.detections as f64 / (s.attempts as f64) < 0.9
        })
        || report.negative_audio_seconds < 3600
        || report.false_activations as f64 * 3600.0 / (report.negative_audio_seconds as f64) > 1.0
    {
        return Err("evaluation gate requires 3 held-out speakers, 10 attempts each, >=90% each, >=1 hour negative audio and <=1 false activation/hour".into());
    }
    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;
    // Synthetic structural fixtures, never evidence of real voice accuracy or consent.
    struct Fixture(std::path::PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn write(root: &Path, file: &str, value: serde_json::Value) {
        fs::write(root.join(file), serde_json::to_vec(&value).unwrap()).unwrap();
    }
    fn fixture() -> Fixture {
        let root = std::env::temp_dir().join(format!("ace-model-fixture-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let frames: Vec<_> = (0..30).map(|_| vec![0.1f32; 13]).collect();
        let reference = serde_json::json!({"name":"ACE","mfcc_size":13,"rms_level":0.1,"threshold":0.52,"avg_threshold":0.22,"samples_features":{"synthetic":frames},"avg_features":null});
        let mut bytes = Vec::new();
        ciborium::into_writer(&reference, &mut bytes).unwrap();
        fs::write(root.join("ace.rpw"), &bytes).unwrap();
        let hash = format!("{:x}", Sha256::digest(&bytes));
        write(
            &root,
            "manifest.json",
            serde_json::json!({"formatVersion":1,"modelVersion":"1.0.0","engine":"rustpotter-3.0.2-ref","file":"ace.rpw","sizeBytes":bytes.len(),"sha256":hash,"redistributionApproved":true,"consentRecord":"consent.json","evaluationRecord":"evaluation.json"}),
        );
        write(
            &root,
            "consent.json",
            serde_json::json!({"redistributionApproved":true,"scope":"model-and-derived-features","recordReference":"SYNTHETIC-TEST-FIXTURE"}),
        );
        write(
            &root,
            "evaluation.json",
            serde_json::json!({"modelSha256":hash,"heldOutSpeakers":[{"participantId":"synthetic1","attempts":10,"detections":9},{"participantId":"synthetic2","attempts":10,"detections":9},{"participantId":"synthetic3","attempts":10,"detections":9}],"negativeAudioSeconds":3600,"falseActivations":1}),
        );
        Fixture(root)
    }
    fn change(root: &Path, file: &str, key: &str, value: serde_json::Value) {
        let mut json: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join(file)).unwrap()).unwrap();
        json[key] = value;
        write(root, file, json);
    }
    #[test]
    fn structurally_valid_synthetic_package_is_accepted() {
        let f = fixture();
        assert!(validate(&f.0).is_ok());
    }
    #[test]
    fn personal_preview_requires_pending_record_and_keeps_integrity_checks() {
        let f = fixture();
        change(
            &f.0,
            "manifest.json",
            "releasePolicy",
            serde_json::json!("personal-preview"),
        );
        assert!(validate(&f.0).is_err());
        change(
            &f.0,
            "evaluation.json",
            "status",
            serde_json::json!("pending"),
        );
        change(
            &f.0,
            "evaluation.json",
            "heldOutSpeakers",
            serde_json::json!([]),
        );
        change(
            &f.0,
            "evaluation.json",
            "negativeAudioSeconds",
            serde_json::json!(0),
        );
        change(
            &f.0,
            "evaluation.json",
            "falseActivations",
            serde_json::json!(0),
        );
        assert!(validate(&f.0).is_ok());
        change(
            &f.0,
            "manifest.json",
            "sha256",
            serde_json::json!("modified"),
        );
        assert!(validate(&f.0).is_err());
    }
    #[test]
    fn missing_model_and_missing_records_fail_closed() {
        let f = fixture();
        fs::remove_file(f.0.join("ace.rpw")).unwrap();
        assert!(validate(&f.0).is_err());
        let f = fixture();
        fs::remove_file(f.0.join("consent.json")).unwrap();
        assert!(validate(&f.0).is_err());
    }
    #[test]
    fn wrong_size_hash_engine_and_version_are_rejected() {
        for (key, value) in [
            ("sizeBytes", serde_json::json!(1)),
            ("sha256", serde_json::json!("bad")),
            ("engine", serde_json::json!("unknown")),
            ("modelVersion", serde_json::json!("2")),
        ] {
            let f = fixture();
            change(&f.0, "manifest.json", key, value);
            assert!(validate(&f.0).is_err());
        }
    }
    #[test]
    fn unapproved_redistribution_is_rejected() {
        let f = fixture();
        change(
            &f.0,
            "manifest.json",
            "redistributionApproved",
            serde_json::json!(false),
        );
        assert!(validate(&f.0).is_err());
    }
    #[test]
    fn insufficient_negative_evaluation_and_false_activations_are_rejected() {
        for (key, value) in [
            ("negativeAudioSeconds", serde_json::json!(30)),
            ("falseActivations", serde_json::json!(2)),
        ] {
            let f = fixture();
            change(&f.0, "evaluation.json", key, value);
            assert!(validate(&f.0).is_err());
        }
    }
    #[test]
    fn resource_path_traversal_is_rejected() {
        let f = fixture();
        change(
            &f.0,
            "manifest.json",
            "file",
            serde_json::json!("../ace.rpw"),
        );
        assert!(validate(&f.0).is_err());
    }
}
