"use client";

import { useRouter } from "next/navigation";
import type { FormEvent } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

export function AccountForm({ initial, org }: { initial: string; org?: string }) {
  const router = useRouter();

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const account = String(data.get("account") ?? "").trim();
    if (account.length === 0) {
      return;
    }
    const params = new URLSearchParams({ account });
    if (org !== undefined && org.length > 0) {
      params.set("org", org);
    }
    router.push(`/usage?${params.toString()}`);
  };

  return (
    <form onSubmit={submit} className="flex gap-2">
      <Input
        name="account"
        defaultValue={initial}
        placeholder="engine account id (uuid)"
        className="w-80"
      />
      <Button type="submit" size="sm">
        View
      </Button>
    </form>
  );
}
