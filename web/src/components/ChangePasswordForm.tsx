import { useState } from "react";
import { toast } from "sonner";
import { Button, Input } from "@swopstar/react-ui";
import { useSetPassword } from "../api/auth/auth";
import { Field } from "./form/Field";

const MIN_PASSWORD_CHARS = 10;

export function ChangePasswordForm() {
  const [current, setCurrent] = useState("");
  const [password, setPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const change = useSetPassword({
    mutation: {
      onSuccess: () => {
        toast.success("Password changed. Other browsers have been signed out.");
        setCurrent("");
        setPassword("");
        setConfirmation("");
      },
      onError: (error) => toast.error(error.message),
    },
  });

  return (
    <form
      className="flex max-w-sm flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault();
        if (password !== confirmation) {
          toast.error("The new passwords don't match.");
          return;
        }
        change.mutate({ data: { password, current_password: current } });
      }}
    >
      <Field id="current-password" label="Current password">
        <Input
          id="current-password"
          type="password"
          autoComplete="current-password"
          required
          value={current}
          onChange={(event) => setCurrent(event.target.value)}
        />
      </Field>
      <Field
        id="change-password"
        label="New password"
        hint={`At least ${MIN_PASSWORD_CHARS} characters.`}
      >
        <Input
          id="change-password"
          type="password"
          autoComplete="new-password"
          required
          minLength={MIN_PASSWORD_CHARS}
          value={password}
          onChange={(event) => setPassword(event.target.value)}
        />
      </Field>
      <Field id="change-confirm" label="Confirm new password">
        <Input
          id="change-confirm"
          type="password"
          autoComplete="new-password"
          required
          value={confirmation}
          onChange={(event) => setConfirmation(event.target.value)}
        />
      </Field>
      <div>
        <Button type="submit" variant="primary" disabled={change.isPending}>
          Change password
        </Button>
      </div>
    </form>
  );
}
