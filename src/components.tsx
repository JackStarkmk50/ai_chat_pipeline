
import type { ReactNode } from "react";

export function Logo() {
  return (
    <span className="inline-flex h-7 w-7 items-center justify-center rounded-lg bg-rvor">
      <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
        <rect x="1.5" y="5" width="2" height="4" rx="1" fill="var(--rv-orink)" />
        <rect x="4.75" y="2.5" width="2" height="9" rx="1" fill="var(--rv-orink)" />
        <rect x="8" y="4" width="2" height="6" rx="1" fill="var(--rv-orink)" />
        <rect x="11.25" y="5.5" width="1.5" height="3" rx="0.75" fill="var(--rv-orink)" />
      </svg>
    </span>
  );
}

export function Header({ title }: { title: string }) {
  return (
    <div className="flex items-center gap-2.5">
      <Logo />
      <span className="text-[15px] font-semibold text-rvtx">{title}</span>
    </div>
  );
}

export type PillKind = "warn" | "ok" | "note";

export function Pill({ kind, children }: { kind: PillKind; children: ReactNode }) {
  const cls =
    kind === "ok"
      ? "bg-rvokbg text-rvok"
      : kind === "warn"
        ? "bg-rvwarnbg text-rvwarn"
        : "bg-rvline/40 text-rvtx2";
  return (
    <span className={`inline-flex items-center rounded-full px-3 py-1 text-[13px] font-medium ${cls}`}>
      {children}
    </span>
  );
}

export function PrimaryButton({
  children,
  onClick,
  disabled,
}: {
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="rounded-full bg-rvor px-6 py-2.5 text-[15px] font-medium text-rvorink transition-opacity disabled:opacity-40"
    >
      {children}
    </button>
  );
}

export function SoftButton({
  children,
  onClick,
  disabled,
}: {
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="rounded-full bg-rvline/50 px-5 py-2.5 text-[15px] font-medium text-rvtx2 transition-opacity disabled:opacity-40"
    >
      {children}
    </button>
  );
}

export function OutlineButton({
  children,
  onClick,
  disabled,
}: {
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="rounded-xl border border-rvedge bg-transparent px-5 py-2 text-[15px] font-medium text-rvtx transition-opacity disabled:opacity-40"
    >
      {children}
    </button>
  );
}

export function GhostButton({
  children,
  onClick,
}: {
  children: ReactNode;
  onClick?: () => void;
}) {
  return (
    <button type="button" onClick={onClick} className="shrink-0 whitespace-nowrap px-3 py-2 text-[15px] font-medium text-rvtx">
      {children}
    </button>
  );
}

export function TextField({
  label,
  value,
  onChange,
  placeholder,
  password,
  action,
}: {
  label: string;
  value: string;
  onChange?: (v: string) => void;
  placeholder?: string;
  password?: boolean;
  action?: ReactNode;
}) {
  return (
    <div>
      <div className="mb-1.5 text-[14px] text-rvtx2">{label}</div>
      <div className="flex items-center gap-2">
        <input
          type={password ? "password" : "text"}
          value={value}
          placeholder={placeholder}
          onChange={(e) => onChange?.(e.target.value)}
          className="w-full rounded-xl border border-rvline bg-rvsurf px-3 py-2.5 text-[15px] text-rvtx outline-none placeholder:text-rvtx2/50 focus:border-rvedge"
        />
        {action}
      </div>
    </div>
  );
}

export function Card({ children, warn }: { children: ReactNode; warn?: boolean }) {
  return (
    <div
      className={`rounded-2xl border bg-rvsurf p-3.5 ${
        warn ? "border-rvwarn/50" : "border-rvline"
      }`}
    >
      {children}
    </div>
  );
}

export function ProgressBar({ done, total }: { done: number; total: number }) {
  const pct = total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;
  return (
    <div>
      <div className="mb-1.5 flex items-baseline justify-between">
        <span className="text-[14px] text-rvtx">Downloading</span>
        <span className="text-[13px] text-rvtx2">{pct}%</span>
      </div>
      <div className="h-1.5 w-full overflow-hidden rounded-full bg-rvline/60">
        <div className="h-full rounded-full bg-rvor transition-all" style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}

export function StepDots({ step }: { step: number }) {
  return (
    <div>
      <div className="flex gap-1.5">
        {[1, 2, 3].map((i) => (
          <span
            key={i}
            className={`h-1 flex-1 rounded-full ${i <= step ? "bg-rvor" : "bg-rvline/60"}`}
          />
        ))}
      </div>
      <div className="mt-2 text-[13px] text-rvtx2">Step {step} of 3</div>
    </div>
  );
}

export function Screen({ children, footer }: { children: ReactNode; footer?: ReactNode }) {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto">{children}</div>
      {footer ? (
        <div className="flex items-center justify-between gap-2 pt-3">{footer}</div>
      ) : null}
    </div>
  );
}
