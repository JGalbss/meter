"use client";

import { useTransition } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { toggleWebhookAction } from "./actions";

function toggleLabel(enabled: boolean): string {
  if (enabled) {
    return "Disable";
  }
  return "Enable";
}

function toggledMessage(enabled: boolean): string {
  if (enabled) {
    return "Enabled";
  }
  return "Disabled";
}

export function WebhookToggle({ id, enabled }: { id: string; enabled: boolean }) {
  const [pending, startTransition] = useTransition();

  const toggle = () =>
    startTransition(async () => {
      const result = await toggleWebhookAction(id, !enabled);
      if (!result.ok) {
        toast.error(result.error);
        return;
      }
      toast.success(toggledMessage(!enabled));
    });

  return (
    <Button variant="outline" size="sm" disabled={pending} onClick={toggle}>
      {toggleLabel(enabled)}
    </Button>
  );
}
