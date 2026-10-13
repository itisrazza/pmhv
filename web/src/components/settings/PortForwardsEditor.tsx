import { Button, Input } from "@swopstar/react-ui";
import { Plus, Trash2 } from "lucide-react";
import type { PortForward, Protocol } from "../../api/schemas";
import { ChoiceSelect } from "../form/ChoiceSelect";

interface PortForwardsEditorProps {
  forwards: PortForward[];
  disabled: boolean;
  onChange: (forwards: PortForward[]) => void;
}

/** Host ports (on 127.0.0.1) forwarded into the guest through QEMU's built-in NAT. */
export function PortForwardsEditor({
  forwards,
  disabled,
  onChange,
}: PortForwardsEditorProps) {
  const replace = (index: number, forward: PortForward) =>
    onChange(
      forwards.map((existing, at) => (at === index ? forward : existing)),
    );

  return (
    <div className="flex flex-col gap-2">
      {forwards.map((forward, index) => (
        <div key={index} className="flex items-center gap-2">
          <div className="w-24">
            <ChoiceSelect<Protocol>
              id={`forward-${index}-protocol`}
              value={forward.protocol}
              disabled={disabled}
              choices={[
                { value: "tcp", label: "TCP" },
                { value: "udp", label: "UDP" },
              ]}
              onChange={(protocol) => replace(index, { ...forward, protocol })}
            />
          </div>
          <span className="text-sm text-muted-foreground">127.0.0.1:</span>
          <Input
            aria-label="Host port"
            className="w-24"
            type="number"
            min={1}
            max={65535}
            value={forward.host_port}
            onChange={(event) =>
              replace(index, {
                ...forward,
                host_port: Number(event.target.value),
              })
            }
          />
          <span className="text-sm text-muted-foreground">→ guest port</span>
          <Input
            aria-label="Guest port"
            className="w-24"
            type="number"
            min={1}
            max={65535}
            value={forward.guest_port}
            onChange={(event) =>
              replace(index, {
                ...forward,
                guest_port: Number(event.target.value),
              })
            }
          />
          <Button
            size="icon-sm"
            title="Remove"
            disabled={disabled}
            onClick={() => onChange(forwards.filter((_, at) => at !== index))}
          >
            <Trash2 />
          </Button>
        </div>
      ))}
      <div>
        <Button
          size="sm"
          disabled={disabled}
          onClick={() =>
            onChange([
              ...forwards,
              { protocol: "tcp", host_port: 2222, guest_port: 22 },
            ])
          }
        >
          <Plus /> Forward a port
        </Button>
      </div>
    </div>
  );
}
