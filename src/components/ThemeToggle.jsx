import { useRef } from "react";
import { Monitor, Moon, Sun } from "lucide-react";

const OPTIONS = [
  { value: "dark", label: "Dark", hint: "Dark mode", Icon: Moon },
  { value: "light", label: "Light", hint: "Light mode", Icon: Sun },
  { value: "system", label: "System", hint: "Match system setting", Icon: Monitor },
];

/** A three-way switch, built as a radio group so arrow keys move between options. */
export function ThemeToggle({ value, onChange }) {
  const buttons = useRef([]);

  function handleKeyDown(event, index) {
    const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[event.key];
    if (!step) return;
    event.preventDefault();
    const next = (index + step + OPTIONS.length) % OPTIONS.length;
    onChange(OPTIONS[next].value);
    buttons.current[next]?.focus();
  }

  return (
    <div role="radiogroup" aria-label="Theme" className="flex items-center gap-0.5 rounded-lg border border-line bg-canvas p-0.5">
      {OPTIONS.map(({ value: option, label, hint, Icon }, index) => {
        const isActive = option === value;
        return (
          <button
            key={option}
            ref={(el) => (buttons.current[index] = el)}
            type="button"
            role="radio"
            aria-checked={isActive}
            aria-label={`${label} theme`}
            title={hint}
            tabIndex={isActive ? 0 : -1}
            onClick={() => onChange(option)}
            onKeyDown={(event) => handleKeyDown(event, index)}
            className={`rounded-md p-1.5 transition-colors ${
              isActive ? "bg-surface text-primary shadow-sm ring-1 ring-line" : "text-ink-faint hover:text-ink"
            }`}
          >
            <Icon className="size-4" aria-hidden />
          </button>
        );
      })}
    </div>
  );
}
