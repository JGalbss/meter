import type { Metadata } from "next";
import type { ReactNode } from "react";

import { Sidebar } from "@/components/sidebar";

import "./globals.css";

export const metadata: Metadata = {
  title: {
    default: "meter docs",
    template: "%s · meter docs",
  },
  description:
    "Documentation for meter — the ledger-first metering, billing, and invoicing engine for AI agents.",
};

export default function RootLayout({ children }: { children: ReactNode }): ReactNode {
  return (
    <html lang="en">
      <body>
        <div className="shell">
          <Sidebar />
          <main className="main">
            <article className="prose">{children}</article>
            <footer className="footer">
              meter — open source, self-hostable metering &amp; billing for AI agents.
            </footer>
          </main>
        </div>
      </body>
    </html>
  );
}
