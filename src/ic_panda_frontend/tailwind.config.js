import { join } from "path";
import { skeleton } from "@skeletonlabs/tw-plugin";

const config = {
  // darkMode: 'class',
  content: [
    "./src/**/*.{html,js,svelte,ts}",
    join(
      require.resolve("@skeletonlabs/skeleton"),
      "../**/*.{html,js,svelte,ts}",
    ),
  ],
  theme: {
    colors: {
      panda: "#11c291",
    },
    extend: {},
  },
  plugins: [
    skeleton({
      themes: { preset: ["skeleton"] },
    }),
  ],
};

export default config;
