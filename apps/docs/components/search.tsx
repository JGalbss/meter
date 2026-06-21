"use client";

import { type ReactNode, useEffect, useRef } from "react";

// Pagefind's UI bundle is produced by the build step (`pagefind --site out`) and served from
// `/pagefind/`. It exists only in a production build, so this component loads it at runtime and
// degrades to an inert box during `next dev`, where the index has not been generated.

interface PagefindUiOptions {
  readonly element: string;
  readonly showSubResults?: boolean;
  readonly showImages?: boolean;
}

interface PagefindUiConstructor {
  new (options: PagefindUiOptions): unknown;
}

declare global {
  interface Window {
    PagefindUI?: PagefindUiConstructor;
  }
}

const STYLE_HREF = "/pagefind/pagefind-ui.css";
const SCRIPT_SRC = "/pagefind/pagefind-ui.js";

function hasStylesheet(): boolean {
  return document.querySelector(`link[href="${STYLE_HREF}"]`) !== null;
}

function loadStylesheet(): void {
  if (hasStylesheet()) {
    return;
  }
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = STYLE_HREF;
  document.head.appendChild(link);
}

function initSearch(): void {
  const PagefindUi = window.PagefindUI;
  if (PagefindUi === undefined) {
    return;
  }
  new PagefindUi({ element: "#docs-search", showSubResults: true, showImages: false });
}

export function Search(): ReactNode {
  const initialized = useRef(false);
  useEffect(() => {
    if (initialized.current) {
      return;
    }
    initialized.current = true;
    loadStylesheet();
    const script = document.createElement("script");
    script.src = SCRIPT_SRC;
    script.async = true;
    script.addEventListener("load", initSearch);
    // No index in dev — fail quiet rather than logging a missing-bundle error.
    script.addEventListener("error", () => undefined);
    document.body.appendChild(script);
  }, []);
  return <div id="docs-search" className="search" data-pagefind-ignore />;
}
