import { createContext, useContext, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { PageHeader } from "@swopstar/react-ui";

export const PageHeaderSlotContext = createContext<HTMLElement | null>(null);

interface PageTitleProps {
  title: ReactNode;
  actions?: ReactNode;
}

/** Renders a page's title and actions into the layout's header bar. */
export function PageTitle({ title, actions }: PageTitleProps) {
  const slot = useContext(PageHeaderSlotContext);
  if (!slot) {
    return null;
  }
  return createPortal(<PageHeader title={title} actions={actions} />, slot);
}
