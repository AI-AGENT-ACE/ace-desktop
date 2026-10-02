import { Settings } from 'lucide-react';

export function ProfileMenu({
  profile,
  onSettings,
}: {
  profile: string;
  onSettings: () => void;
}) {
  return (
    <div className="profile profile-direct">
      <div className="avatar" aria-hidden="true">
        {Array.from(profile.trim())[0]?.toLocaleUpperCase() || 'A'}
      </div>
      <strong title={profile}>{profile}</strong>
      <button
        type="button"
        className="icon-button profile-settings"
        aria-label="설정 열기"
        title="설정"
        onClick={onSettings}
      >
        <Settings size={17} />
      </button>
    </div>
  );
}
