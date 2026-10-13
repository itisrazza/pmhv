import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@swopstar/react-ui";

export interface Choice<T extends string> {
  value: T;
  label: string;
  disabled?: boolean;
}

interface ChoiceSelectProps<T extends string> {
  id: string;
  value: T;
  choices: readonly Choice<T>[];
  onChange: (value: T) => void;
}

export function ChoiceSelect<T extends string>({
  id,
  value,
  choices,
  onChange,
}: ChoiceSelectProps<T>) {
  return (
    <Select value={value} onValueChange={(next) => onChange(next as T)}>
      <SelectTrigger id={id} className="w-full">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {choices.map((choice) => (
          <SelectItem
            key={choice.value}
            value={choice.value}
            disabled={choice.disabled}
          >
            {choice.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
