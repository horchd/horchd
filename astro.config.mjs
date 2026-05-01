// @ts-check
import { defineConfig } from "astro/config";

export default defineConfig({
  site: "https://horchd.xyz",
  trailingSlash: "ignore",
  build: {
    inlineStylesheets: "auto",
  },
});
