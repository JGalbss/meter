import createMDX from "@next/mdx";

const withMDX = createMDX();

/** @type {import('next').NextConfig} */
const config = {
  pageExtensions: ["ts", "tsx", "md", "mdx"],
  // The docs are fully static; export to plain HTML so the site can be served from any static host
  // (and indexed by Pagefind for client-side search). See `deploy/docs.Dockerfile`.
  output: "export",
};

export default withMDX(config);
