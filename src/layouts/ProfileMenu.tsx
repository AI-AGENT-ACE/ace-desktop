import { useEffect, useId, useRef, useState } from 'react';
import { ChevronUp, Settings, Trash2 } from 'lucide-react';

export function ProfileMenu({
  profile,
  onSettings,
  onTrash,
}: {
  profile: string;
  onSettings: () => void;
  onTrash: () => void;
}) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    menu.current?.querySelector<HTMLButtonElement>('button')?.focus();
    const outside = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener('pointerdown', outside);
    return () => document.removeEventListener('pointerdown', outside);
  }, [open]);
  const select = (action: () => void) => {
    setOpen(false);
    trigger.current?.focus();
    action();
  };
  return (
    <div
      className="profile-menu-anchor"
      ref={root}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOpen(false);
      }}
      onKeyDown={(event) => {
        if (event.key === 'Escape' && open) {
          event.preventDefault();
          setOpen(false);
          trigger.current?.focus();
        }
      }}
    >
      {open && (
        <div
          id={id}
          className="profile-menu"
          role="menu"
          aria-label="사용자 메뉴"
          ref={menu}
          onKeyDown={(event) => {
            const buttons = Array.from(
              event.currentTarget.querySelectorAll<HTMLButtonElement>('button'),
            );
            const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
            if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
              event.preventDefault();
              const next =
                event.key === 'Home'
                  ? 0
                  : event.key === 'End'
                    ? buttons.length - 1
                    : (index + (event.key === 'ArrowDown' ? 1 : -1) + buttons.length) %
                      buttons.length;
              buttons[next]?.focus();
            }
          }}
        >
          <button type="button" role="menuitem" onClick={() => select(onSettings)}>
            <Settings size={16} />
            설정
          </button>
          <button type="button" role="menuitem" onClick={() => select(onTrash)}>
            <Trash2 size={16} />
            휴지통
          </button>
        </div>
      )}
      <button
        type="button"
        className="profile profile-trigger"
        ref={trigger}
        aria-label="사용자 프로필 메뉴"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        onClick={() => setOpen(!open)}
        onKeyDown={(event) => {
          if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
            event.preventDefault();
            setOpen(true);
          }
        }}
      >
        <div className="avatar" aria-hidden="true">
          {Array.from(profile.trim())[0]?.toLocaleUpperCase() || 'A'}
        </div>
        <strong title={profile}>{profile}</strong>
        <ChevronUp size={16} className="profile-chevron" aria-hidden="true" />
      </button>
    </div>
  );
}
