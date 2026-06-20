"use client";

import { type FormEvent, useState, useTransition } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { createWebhookAction } from "./actions";

function parseEventTypes(raw: string): string[] {
  return raw
    .split(",")
    .map((entry) => entry.trim())
    .filter((entry) => entry.length > 0);
}

export function RegisterWebhookDialog({ orgId }: { orgId: string }) {
  const [open, setOpen] = useState(false);
  const [pending, startTransition] = useTransition();

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const url = String(data.get("url") ?? "");
    const secret = String(data.get("secret") ?? "");
    const eventTypes = parseEventTypes(String(data.get("eventTypes") ?? ""));
    startTransition(async () => {
      const result = await createWebhookAction({ orgId, url, secret, eventTypes });
      if (!result.ok) {
        toast.error(result.error);
        return;
      }
      toast.success("Webhook registered");
      setOpen(false);
    });
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button size="sm" />}>Register webhook</DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Register webhook</DialogTitle>
          <DialogDescription>
            Deliveries are signed with HMAC-SHA256 over your secret. Leave events blank for all.
          </DialogDescription>
        </DialogHeader>
        <form onSubmit={submit} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="url">Endpoint URL</Label>
            <Input id="url" name="url" type="url" placeholder="https://example.com/hooks/meter" required />
          </div>
          <div className="space-y-2">
            <Label htmlFor="secret">Signing secret</Label>
            <Input id="secret" name="secret" placeholder="whsec_…" required />
          </div>
          <div className="space-y-2">
            <Label htmlFor="eventTypes">Event types (comma-separated)</Label>
            <Input id="eventTypes" name="eventTypes" placeholder="budget, credit" />
          </div>
          <DialogFooter>
            <Button type="submit" disabled={pending}>
              Register
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
