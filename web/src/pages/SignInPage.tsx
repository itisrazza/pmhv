import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Alert, AlertDescription, Button, Input } from "@swopstar/react-ui";
import { getGetSessionQueryKey, useLogin } from "../api/auth/auth";
import { AuthCard } from "../components/AuthCard";
import { Field } from "../components/form/Field";

export function SignInPage() {
  const [password, setPassword] = useState("");
  const queryClient = useQueryClient();
  const login = useLogin({
    mutation: {
      onSuccess: (status) =>
        queryClient.setQueryData(getGetSessionQueryKey(), status),
    },
  });

  return (
    <AuthCard
      title="Sign in"
      description="On first launch, pmhvd prints a temporary password to its log and saves it as initial-password.txt in its data folder."
      onSubmit={() => login.mutate({ data: { password } })}
    >
      <Field id="password" label="Password">
        <Input
          id="password"
          type="password"
          autoComplete="current-password"
          autoFocus
          required
          value={password}
          onChange={(event) => setPassword(event.target.value)}
        />
      </Field>
      {login.isError && (
        <Alert variant="destructive">
          <AlertDescription>{login.error.message}</AlertDescription>
        </Alert>
      )}
      <Button type="submit" variant="primary" disabled={login.isPending}>
        Sign in
      </Button>
    </AuthCard>
  );
}
