export function Slider({
  label,
  value,
  min,
  max,
  step = 1,
  unit = "",
  onChange,
  hint,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  unit?: string;
  onChange: (v: number) => void;
  hint?: string;
}) {
  const fill = ((value - min) / (max - min)) * 100;
  return (
    <div className="space-y-1.5">
      <div className="flex items-baseline justify-between">
        <label className="text-[12.5px] font-medium opacity-80">{label}</label>
        <span className="font-mono-console text-[12px]" style={{ color: "var(--accent)" }}>
          {value}
          {unit}
        </span>
      </div>
      <input
        type="range"
        className="rng"
        min={min}
        max={max}
        step={step}
        value={value}
        style={{ ["--fill" as string]: `${fill}%` }}
        onChange={(e) => onChange(Number(e.target.value))}
      />
      {hint && <p className="text-[11px] opacity-50">{hint}</p>}
    </div>
  );
}

export function Skeleton({ className = "" }: { className?: string }) {
  return <div className={`skel ${className}`} />;
}

export function CardGridSkeleton({ count = 8 }: { count?: number }) {
  return (
    <div className="grid grid-cols-2 gap-3 xl:grid-cols-3">
      {Array.from({ length: count }).map((_, i) => (
        <div key={i} className="glass p-4">
          <div className="flex gap-3">
            <Skeleton className="size-14 shrink-0" />
            <div className="flex-1 space-y-2">
              <Skeleton className="h-4 w-2/3" />
              <Skeleton className="h-3 w-full" />
              <Skeleton className="h-3 w-1/2" />
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}
