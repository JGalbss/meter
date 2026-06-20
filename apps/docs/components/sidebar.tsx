"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import type { ReactNode } from "react";

type NavItem = { href: string; label: string };

const NAV: ReadonlyArray<NavItem> = [
  { href: "/", label: "Overview" },
  { href: "/concepts", label: "Concepts" },
  { href: "/api", label: "API reference" },
  { href: "/api/control-plane", label: "Control plane API" },
  { href: "/sdks", label: "SDKs & downloads" },
  { href: "/self-host", label: "Self-host" },
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
    <aside className="sidebar">
      <Link href="/" className="brand">
        meter
      </Link>
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
