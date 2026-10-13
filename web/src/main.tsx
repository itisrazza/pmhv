import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import {
  QueryCache,
  QueryClient,
  QueryClientProvider,
} from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { ThemeProvider } from "@swopstar/react-ui";
import "@swopstar/react-ui/style.css";
import "./index.css";
import { getGetSessionQueryKey } from "./api/auth/auth";
import { ApiRequestError } from "./lib/api-client";
import { router } from "./router";

const BRAND_COLOUR = "#00B191";

const queryClient: QueryClient = new QueryClient({
  queryCache: new QueryCache({
    // A request bounced for lack of a session: recheck it, which brings up the sign-in screen.
    onError: (error) => {
      if (error instanceof ApiRequestError && error.status === 401) {
        void queryClient.invalidateQueries({
          queryKey: getGetSessionQueryKey(),
        });
      }
    },
  }),
  defaultOptions: {
    queries: {
      retry: (failures, error) =>
        !(error instanceof ApiRequestError && error.status < 500) &&
        failures < 3,
    },
  },
});

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider seedColor={BRAND_COLOUR} mode="auto">
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </ThemeProvider>
  </StrictMode>,
);
