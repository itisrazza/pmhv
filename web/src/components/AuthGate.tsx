import type { ReactNode } from "react";
import { useGetSession } from "../api/auth/auth";
import { useMachineEvents } from "../lib/events";
import { SetPasswordPage } from "../pages/SetPasswordPage";
import { SignInPage } from "../pages/SignInPage";

function SignedIn({ children }: { children: ReactNode }) {
  useMachineEvents();
  return children;
}

/** Shows the app only to a signed-in browser, and sends a temporary-password session to setup. */
export function AuthGate({ children }: { children: ReactNode }) {
  const session = useGetSession();
  if (session.isError) {
    return (
      <div className="flex min-h-screen items-center justify-center p-6 text-center">
        <p>
          Can't reach pmhvd: {session.error.message}. Check it's running, then
          reload.
        </p>
      </div>
    );
  }
  if (!session.data) {
    return null;
  }
  if (!session.data.authenticated) {
    return <SignInPage />;
  }
  if (session.data.setup_required) {
    return <SetPasswordPage />;
  }
  return <SignedIn>{children}</SignedIn>;
}
