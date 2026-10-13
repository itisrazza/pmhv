import type { FormEvent, ReactNode } from "react";

interface AuthCardProps {
  title: string;
  description: string;
  onSubmit: () => void;
  children: ReactNode;
}

export function AuthCard({
  title,
  description,
  onSubmit,
  children,
}: AuthCardProps) {
  const submit = (event: FormEvent) => {
    event.preventDefault();
    onSubmit();
  };
  return (
    <div className="flex min-h-screen items-center justify-center bg-background p-6">
      <form
        onSubmit={submit}
        className="flex w-full max-w-sm flex-col gap-4 rounded-lg border border-border bg-card p-6 text-card-foreground"
      >
        <div className="flex items-center gap-2">
          <img src="/favicon.svg" width={32} height={32} alt="" />
          <span className="text-xl font-extrabold">pmhv</span>
        </div>
        <div className="flex flex-col gap-1">
          <h1 className="text-lg font-semibold">{title}</h1>
          <p className="text-sm text-muted-foreground">{description}</p>
        </div>
        {children}
      </form>
    </div>
  );
}
