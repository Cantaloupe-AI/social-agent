import { Textarea } from "@/components/ui/textarea";

export interface ThoughtsFieldProps {
  value: string;
  onChange: (v: string) => void;
}

export function ThoughtsField({ value, onChange }: ThoughtsFieldProps) {
  return (
    <div className="flex flex-col gap-2">
      <label
        htmlFor="thoughts"
        className="text-xs uppercase tracking-wider text-muted-foreground"
      >
        Anything worth remembering?
      </label>
      <Textarea
        id="thoughts"
        value={value}
        onChange={(e) => onChange(e.currentTarget.value)}
        placeholder="Ideas, frustrations, half-thoughts, post candidates…"
        rows={4}
        className="min-h-[6rem] max-h-[16rem] font-sans text-sm leading-6 resize-none overflow-y-auto"
      />
    </div>
  );
}
