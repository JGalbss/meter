"use client";

import { useTransition } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { evaluateAction } from "./actions";

export function EvaluateButton({ orgId }: { orgId: string }) {
  const [pending, startTransition] = useTransition();

  const evaluate = () =>
    startTransition(async () => {
      const result = await evaluateAction(orgId);
      if (!result.ok) {
        toast.error(result.error);
        return;
      }
      toast.success(`Evaluated ${result.evaluated} rule(s), raised ${result.raised} alert(s)`);
    });

  return (
    <Button size="sm" disabled={pending} onClick={evaluate}>
      Evaluate now
    </Button>
  );
}
