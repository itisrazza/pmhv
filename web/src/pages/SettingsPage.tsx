import { Alert, AlertDescription, AlertTitle } from "@swopstar/react-ui";
import { useGetHost } from "../api/host/host";
import type { HostInfo } from "../api/schemas";

function HostDetails({ host }: { host: HostInfo }) {
  const rows = [
    { label: "pmhvd version", value: host.version },
    { label: "Operating system", value: host.os },
    { label: "Architecture", value: host.arch },
  ];
  return (
    <dl className="grid grid-cols-[max-content_1fr] gap-x-8 gap-y-2">
      {rows.map(({ label, value }) => (
        <div key={label} className="contents">
          <dt className="font-semibold">{label}</dt>
          <dd className="text-right">{value}</dd>
        </div>
      ))}
    </dl>
  );
}

export function SettingsPage() {
  const host = useGetHost();

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-4 p-6">
      <h2 className="text-base font-semibold">About this host</h2>
      {host.isPending && (
        <p className="text-sm text-muted-foreground">Loading…</p>
      )}
      {host.isError && (
        <Alert variant="destructive">
          <AlertTitle>Couldn't reach pmhvd</AlertTitle>
          <AlertDescription>{host.error.message}</AlertDescription>
        </Alert>
      )}
      {host.isSuccess && <HostDetails host={host.data.data} />}
    </div>
  );
}
