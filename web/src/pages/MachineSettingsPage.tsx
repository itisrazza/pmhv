import { useState, type ReactNode } from "react";
import { useNavigate } from "@tanstack/react-router";
import { toast } from "sonner";
import {
  Alert,
  AlertDescription,
  Button,
  Checkbox,
  Input,
  Label,
  Textarea,
} from "@swopstar/react-ui";
import { useListImages } from "../api/images/images";
import { useGetMachine, useUpdateMachine } from "../api/machines/machines";
import type {
  Arch,
  ConsoleKind,
  GuestOs,
  Machine,
  MachineSpec,
  NetworkSpec,
} from "../api/schemas";
import { ChoiceSelect, type Choice } from "../components/form/ChoiceSelect";
import { Field } from "../components/form/Field";
import { PageTitle } from "../components/PageHeaderSlot";
import { DiskListEditor } from "../components/settings/DiskListEditor";
import { PortForwardsEditor } from "../components/settings/PortForwardsEditor";
import {
  archLabels,
  consoleLabels,
  guestOsLabels,
  networkLabels,
} from "../lib/labels";

function choicesFrom<T extends string>(labels: Record<T, string>): Choice<T>[] {
  return (Object.keys(labels) as T[]).map((value) => ({
    value,
    label: labels[value],
  }));
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-4">
      <h2 className="font-semibold">{title}</h2>
      {children}
    </section>
  );
}

function SettingsForm({ machine }: { machine: Machine }) {
  const [spec, setSpec] = useState<MachineSpec>(machine.spec);
  const images = useListImages();
  const navigate = useNavigate();
  const back = () =>
    navigate({ to: "/machines/$id", params: { id: machine.id } });
  const save = useUpdateMachine({
    mutation: {
      onSuccess: () => {
        toast.success("Saved");
        void back();
      },
      onError: (error) => toast.error(error.message),
    },
  });

  const locked = machine.state !== "stopped";
  const update = (changes: Partial<MachineSpec>) =>
    setSpec((current) => ({ ...current, ...changes }));
  const allImages = images.data?.flatMap((group) => group.images) ?? [];

  return (
    <>
      <PageTitle
        title={`${machine.spec.name} settings`}
        actions={
          <>
            <Button variant="outline" onClick={() => void back()}>
              Cancel
            </Button>
            <Button
              variant="primary"
              disabled={save.isPending}
              onClick={() => save.mutate({ id: machine.id, data: spec })}
            >
              Save
            </Button>
          </>
        }
      />
      <div className="mx-auto flex w-full max-w-3xl flex-col gap-8 p-6">
        {locked && (
          <Alert>
            <AlertDescription>
              The machine is running, so only its notes can be changed. Stop it
              to edit the rest.
            </AlertDescription>
          </Alert>
        )}

        <Section title="General">
          <Field id="name" label="Name">
            <Input
              id="name"
              disabled={locked}
              value={spec.name}
              onChange={(event) => update({ name: event.target.value })}
            />
          </Field>
          <Field id="guest-os" label="Operating system">
            <ChoiceSelect<GuestOs>
              id="guest-os"
              disabled={locked}
              value={spec.guest_os}
              choices={choicesFrom(guestOsLabels)}
              onChange={(guest_os) => update({ guest_os })}
            />
          </Field>
          <Field id="notes" label="Notes">
            <Textarea
              id="notes"
              rows={5}
              value={spec.notes ?? ""}
              onChange={(event) => update({ notes: event.target.value })}
            />
          </Field>
        </Section>

        <fieldset disabled={locked} className="flex flex-col gap-8">
          <Section title="Hardware">
            <div className="grid grid-cols-2 gap-4">
              <Field id="arch" label="Architecture">
                <ChoiceSelect<Arch>
                  id="arch"
                  value={spec.arch}
                  choices={choicesFrom(archLabels)}
                  onChange={(arch) =>
                    update({
                      arch,
                      firmware: arch === "x86_64" ? spec.firmware : "uefi",
                    })
                  }
                />
              </Field>
              <Field id="firmware" label="Firmware">
                <ChoiceSelect
                  id="firmware"
                  value={spec.firmware}
                  choices={[
                    { value: "uefi", label: "UEFI" },
                    {
                      value: "bios",
                      label: "Legacy BIOS (x86 only)",
                      disabled: spec.arch !== "x86_64",
                    },
                  ]}
                  onChange={(firmware) => update({ firmware })}
                />
              </Field>
              <Field id="cores" label="CPU cores">
                <Input
                  id="cores"
                  type="number"
                  min={1}
                  value={spec.cpu_cores}
                  onChange={(event) =>
                    update({ cpu_cores: Number(event.target.value) })
                  }
                />
              </Field>
              <Field id="memory" label="Memory (MiB)">
                <Input
                  id="memory"
                  type="number"
                  min={64}
                  step={256}
                  value={spec.memory_mib}
                  onChange={(event) =>
                    update({ memory_mib: Number(event.target.value) })
                  }
                />
              </Field>
              <Field id="console" label="Console">
                <ChoiceSelect<ConsoleKind>
                  id="console"
                  value={spec.console}
                  choices={choicesFrom(consoleLabels)}
                  onChange={(console) => update({ console })}
                />
              </Field>
              <Field id="network" label="Network">
                <ChoiceSelect<NetworkSpec>
                  id="network"
                  value={spec.network}
                  choices={choicesFrom(networkLabels)}
                  onChange={(network) => update({ network })}
                />
              </Field>
            </div>
            <div className="flex items-center gap-2">
              <Checkbox
                id="tpm"
                checked={spec.tpm ?? false}
                onCheckedChange={(checked) => update({ tpm: checked === true })}
              />
              <Label htmlFor="tpm">TPM 2.0 (needs swtpm on the host)</Label>
            </div>
          </Section>

          {spec.backend === "qemu" && spec.network === "nat" && (
            <Section title="Port forwards">
              <PortForwardsEditor
                forwards={spec.port_forwards ?? []}
                disabled={locked}
                onChange={(port_forwards) => update({ port_forwards })}
              />
            </Section>
          )}

          <Section title="Disks">
            <DiskListEditor
              arch={spec.arch}
              disks={spec.disks ?? []}
              images={allImages}
              disabled={locked}
              onChange={(disks) => update({ disks })}
            />
          </Section>
        </fieldset>
      </div>
    </>
  );
}

export function MachineSettingsPage({ id }: { id: string }) {
  const machine = useGetMachine(id);
  if (machine.isError) {
    return (
      <Alert variant="destructive" className="m-6 w-auto">
        <AlertDescription>{machine.error.message}</AlertDescription>
      </Alert>
    );
  }
  // Keyed by ID so switching machines resets the draft.
  return machine.data ? (
    <SettingsForm key={machine.data.id} machine={machine.data} />
  ) : null;
}
