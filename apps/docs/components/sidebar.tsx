"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import type { ReactNode } from "react";

import { Search } from "./search";

type NavItem = { href: string; label: string };

const NAV: ReadonlyArray<NavItem> = [
  { href: "/", label: "Overview" },
  { href: "/quickstart", label: "Quickstart" },
  { href: "/concepts", label: "Concepts" },
  { href: "/pricing", label: "Pricing" },
  { href: "/api", label: "API reference" },
  { href: "/api/engine", label: "Engine API" },
  { href: "/api/control-plane", label: "Control plane API" },
  { href: "/errors", label: "Errors" },
  { href: "/sdks", label: "SDKs and downloads" },
  { href: "/migrating", label: "Migrating" },
  { href: "/self-host", label: "Self-host" },
  { href: "/glossary", label: "Glossary" },
];

function isActive(pathname: string, href: string): boolean {
  if (href === "/") {
    return pathname === "/";
  }
  return pathname === href || pathname.startsWith(`${href}/`);
}

export function Sidebar(): ReactNode {
  const pathname = usePathname();
  return (
    <aside className="sidebar" data-pagefind-ignore>
      <Link href="/" className="brand">
        meter
      </Link>
      <Search />
      <nav className="nav">
        {NAV.map((item) => (
          <Link
            key={item.href}
            href={item.href}
            className="nav-link"
            aria-current={isActive(pathname, item.href) ? "page" : undefined}
          >
            {item.label}
          </Link>
        ))}
      </nav>
      <a className="repo-link" href="https://github.com/JGalbss/meter">
        GitHub ↗
      </a>
    </aside>
  );
}
