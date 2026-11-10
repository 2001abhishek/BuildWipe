import { useEffect, useRef } from "react";

const BUTTON_VARIANTS = {
  primary: "bg-primary text-on-primary hover:bg-primary-hover",
  danger: "bg-danger text-on-danger hover:bg-danger-hover",
  ghost: "border border-line text-ink-muted hover:bg-raised hover:text-ink",
};

export function Button({ variant = "ghost", className = "", children, ...props }) {
  return (
    <button
      type="button"
      className={`inline-flex items-center justify-center gap-2 rounded-lg px-3.5 py-2 text-sm font-semibold transition-colors disabled:opacity-40 disabled:hover:bg-transparent ${
        variant === "ghost" ? "" : "disabled:bg-raised disabled:text-ink-faint disabled:hover:bg-raised"
      } ${BUTTON_VARIANTS[variant]} ${className}`}
      {...props}
    >
      {children}
    </button>
  );
}

export function Checkbox({ checked, indeterminate = false, label, onChange, ...props }) {
  const ref = useRef(null);
  useEffect(() => {
    ref.current.indeterminate = indeterminate;
  }, [indeterminate]);

  return (
    <input
      ref={ref}
      type="checkbox"
      aria-label={label}
      checked={checked}
      onChange={(event) => onChange(event.target.checked)}
      onClick={(event) => event.stopPropagation()}
      {...props}
    />
  );
}

export function SectionLabel({ children }) {
  return <h2 className="text-xs font-bold tracking-wider text-ink-faint uppercase">{children}</h2>;
}

/** A path that shortens from the middle, so the last segment always stays readable. */
export function PathText({ path, display = path, className = "" }) {
  const split = Math.max(display.lastIndexOf("\\", display.length - 2), display.lastIndexOf("/", display.length - 2)) + 1;
  return (
    <span className={`flex min-w-0 font-secondary whitespace-pre select-text ${className}`} title={path}>
      <span className="min-w-0 truncate">{display.slice(0, split)}</span>
      <span className="max-w-3/5 shrink-0 truncate">{display.slice(split)}</span>
    </span>
  );
}

export function Dot({ className }) {
  return <span aria-hidden className={`inline-block size-2 shrink-0 rounded-full ${className}`} />;
}

/** A native modal dialog, which brings focus trapping and Escape handling with it. */
export function Modal({ title, onClose, children }) {
  const ref = useRef(null);
  useEffect(() => {
    const dialog = ref.current;
    if (!dialog.open) dialog.showModal();
    return () => dialog.close();
  }, []);

  return (
    <dialog
      ref={ref}
      aria-label={title}
      onCancel={(event) => {
        event.preventDefault();
        onClose?.();
      }}
      className="m-auto max-h-[85vh] w-full max-w-xl overflow-y-auto rounded-2xl border border-line bg-surface p-6 text-ink backdrop:bg-scrim"
    >
      {children}
    </dialog>
  );
}
