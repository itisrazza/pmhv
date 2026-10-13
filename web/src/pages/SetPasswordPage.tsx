import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Alert, AlertDescription, Button, Input } from "@swopstar/react-ui";
import { getGetSessionQueryKey, useSetPassword } from "../api/auth/auth";
import { AuthCard } from "../components/AuthCard";
import { Field } from "../components/form/Field";

const MIN_PASSWORD_CHARS = 10;

export function SetPasswordPage() {
  const [password, setPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [mismatch, setMismatch] = useState(false);
  const queryClient = useQueryClient();
  const setPasswordMutation = useSetPassword({
    mutation: {
      onSuccess: (status) =>
        queryClient.setQueryData(getGetSessionQueryKey(), status),
    },
  });

  const submit = () => {
    if (password !== confirmation) {
      setMismatch(true);
      return;
    }
    setMismatch(false);
    setPasswordMutation.mutate({ data: { password } });
  };

  return (
    <AuthCard
      title="Choose a password"
      description="You signed in with the temporary password. Set your own to continue; the temporary one stops working."
      onSubmit={submit}
    >
      <Field
        id="new-password"
        label="New password"
        hint={`At least ${MIN_PASSWORD_CHARS} characters.`}
      >
        <Input
          id="new-password"
          type="password"
          autoComplete="new-password"
          autoFocus
          required
          minLength={MIN_PASSWORD_CHARS}
          value={password}
          onChange={(event) => setPassword(event.target.value)}
        />
      </Field>
      <Field id="confirm-password" label="Confirm password">
        <Input
          id="confirm-password"
          type="password"
          autoComplete="new-password"
          required
          value={confirmation}
          onChange={(event) => setConfirmation(event.target.value)}
        />
      </Field>
      {(mismatch || setPasswordMutation.isError) && (
        <Alert variant="destructive">
          <AlertDescription>
            {mismatch
              ? "The passwords don't match."
              : setPasswordMutation.error?.message}
          </AlertDescription>
        </Alert>
      )}
      <Button
        type="submit"
        variant="primary"
        disabled={setPasswordMutation.isPending}
      >
        Set password
      </Button>
    </AuthCard>
  );
}
