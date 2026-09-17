import { useEffect, useRef, type ReactNode } from 'react';
import { X } from 'lucide-react';
export function Modal({ title, onClose, children }: { title: string; onClose: () => void; children: ReactNode }) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => { const previous = document.activeElement as HTMLElement; ref.current?.showModal(); return () => { ref.current?.close(); previous?.focus(); }; }, []);
  return <dialog ref={ref} className="modal" onCancel={e => { e.preventDefault(); onClose(); }} onClick={e => { if (e.target === e.currentTarget) onClose(); }}>
    <div className="modal-inner"><header><h2>{title}</h2><button className="icon-button" aria-label="닫기" onClick={onClose}><X size={19}/></button></header>{children}</div>
  </dialog>;
}
