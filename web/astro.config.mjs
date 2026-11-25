// The site of tech design 0065: static pages, served on the one origin that Keycloak, the API
// and the bucket allow.
import { defineConfig } from "astro/config";

export default defineConfig({
  output: "static",
  server: { host: "localhost", port: 4321 },
  // Another port would be another origin, so the dev server stops when 4321 is in use.
  vite: { server: { strictPort: true } },
  devToolbar: { enabled: false },
});
