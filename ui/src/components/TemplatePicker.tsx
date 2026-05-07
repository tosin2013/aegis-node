import { useState } from "react";
import { ChevronDown, Library } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  getAllTemplates,
  groupByDifficulty,
} from "@/templates/loader";
import type { Template, TemplateDifficulty } from "@/templates/types";

interface TemplatePickerProps {
  onSelect: (template: Template) => void;
  className?: string;
}

const DIFFICULTY_LABELS: Record<TemplateDifficulty, string> = {
  starter: "Starter",
  intermediate: "Intermediate",
  advanced: "Advanced",
};

const DIFFICULTY_ORDER: ReadonlyArray<TemplateDifficulty> = [
  "starter",
  "intermediate",
  "advanced",
];

export function TemplatePicker({ onSelect, className }: TemplatePickerProps) {
  const [open, setOpen] = useState(false);
  const total = getAllTemplates().length;
  const grouped = groupByDifficulty();

  return (
    <div className={cn("relative", className)}>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        aria-haspopup="listbox"
        className="inline-flex items-center gap-1.5 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-elev)] px-3 py-1.5 text-sm transition-colors hover:border-accent hover:text-accent"
      >
        <Library className="h-4 w-4" aria-hidden="true" />
        <span>Load Template</span>
        <span className="font-mono text-xs text-muted">({total})</span>
        <ChevronDown
          className={cn(
            "h-4 w-4 transition-transform",
            open && "rotate-180",
          )}
          aria-hidden="true"
        />
      </button>

      {open && (
        <>
          {/* Click-away catcher; closes the panel when the operator
              clicks outside it. */}
          <button
            type="button"
            aria-label="Close template picker"
            onClick={() => setOpen(false)}
            className="fixed inset-0 z-10 cursor-default bg-transparent"
          />
          <div
            role="listbox"
            aria-label="Manifest templates"
            className="absolute right-0 z-20 mt-2 w-[28rem] max-h-[28rem] overflow-y-auto rounded-md border border-[var(--color-border)] bg-[var(--color-bg-elev)] shadow-xl"
          >
            {DIFFICULTY_ORDER.map((diff) => {
              const items = grouped[diff];
              if (items.length === 0) return null;
              return (
                <section
                  key={diff}
                  className="border-b border-[var(--color-border)] last:border-0"
                >
                  <h3 className="sticky top-0 bg-[var(--color-bg-elev)] px-4 py-2 text-xs font-semibold uppercase tracking-wider text-muted">
                    {DIFFICULTY_LABELS[diff]}{" "}
                    <span className="font-normal text-[10px]">
                      ({items.length})
                    </span>
                  </h3>
                  <ul>
                    {items.map((t) => (
                      <li key={t.id}>
                        <button
                          type="button"
                          onClick={() => {
                            onSelect(t);
                            setOpen(false);
                          }}
                          className="block w-full px-4 py-2.5 text-left text-sm transition-colors hover:bg-[var(--color-bg)]"
                        >
                          <div className="flex items-baseline justify-between gap-3">
                            <span className="font-medium text-[var(--color-fg)]">
                              {t.title}
                            </span>
                            <span className="font-mono text-[10px] text-muted">
                              {t.category}
                            </span>
                          </div>
                          <p className="mt-0.5 line-clamp-2 text-xs text-muted">
                            {t.pain}
                          </p>
                          {t.adrs.length > 0 && (
                            <div className="mt-1.5 flex gap-1">
                              {t.adrs.map((adr) => (
                                <span
                                  key={adr}
                                  className="rounded bg-[var(--color-bg)] px-1.5 py-0.5 font-mono text-[10px] text-accent"
                                >
                                  ADR-{String(adr).padStart(3, "0")}
                                </span>
                              ))}
                            </div>
                          )}
                        </button>
                      </li>
                    ))}
                  </ul>
                </section>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}
