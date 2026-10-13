import { defineConfig } from "orval";

export default defineConfig({
  pmhv: {
    input: "./openapi.json",
    output: {
      mode: "tags-split",
      target: "src/api",
      schemas: "src/api/schemas",
      client: "react-query",
      clean: true,
      override: {
        mutator: { path: "src/lib/api-client.ts", name: "apiClient" },
      },
    },
  },
});
