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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { createAlertRuleAction } from "./actions";

const SCOPES = ["org", "team", "user", "product"];
const METRICS = ["budget", "credit", "spend"];
const ACTIONS = ["notify", "webhook", "enforce"];

interface Extras {
  accountId?: string;
  creditLimit?: number;
  windowDays?: number;
}

export function CreateAlertRuleDialog({ orgId }: { orgId: string }) {
  const [open, setOpen] = useState(false);
  const [pending, startTransition] = useTransition();
  const [scope, setScope] = useState("org");
  const [metric, setMetric] = useState("budget");
  const [action, setAction] = useState("notify");

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const name = String(data.get("name") ?? "");
    const threshold = Number(data.get("threshold") ?? 0);
    const accountId = String(data.get("accountId") ?? "").trim();
    const creditLimit = String(data.get("creditLimit") ?? "").trim();
    const windowDays = String(data.get("windowDays") ?? "").trim();

    const extras: Extras = {};
    if (accountId.length > 0) {
      extras.accountId = accountId;
    }
    if (creditLimit.length > 0) {
      extras.creditLimit = Number(creditLimit);
    }
    if (windowDays.length > 0) {
      extras.windowDays = Number(windowDays);
    }

    startTransition(async () => {
      const result = await createAlertRuleAction({
        orgId,
        name,
        scope,
        metric,
        action,
        threshold,
        ...extras,
      });
      if (!result.ok) {
        toast.error(result.error);
        return;
      }
      toast.success("Alert rule created");
      setOpen(false);
    });
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button size="sm" />}>New rule</DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>New alert rule</DialogTitle>
          <DialogDescription>
            Budget rules watch engine-account usage against a credit limit.
          </DialogDescription>
        </DialogHeader>
        <form onSubmit={submit} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="name">Name</Label>
            <Input id="name" name="name" placeholder="Monthly cap" required />
          </div>
          <div className="grid grid-cols-3 gap-3">
            <div className="space-y-2">
              <Label>Scope</Label>
              <Select value={scope} onValueChange={(value) => setScope(String(value))}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {SCOPES.map((option) => (
                    <SelectItem key={option} value={option}>
                      {option}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label>Metric</Label>
              <Select value={metric} onValueChange={(value) => setMetric(String(value))}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {METRICS.map((option) => (
                    <SelectItem key={option} value={option}>
                      {option}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label>Action</Label>
              <Select value={action} onValueChange={(value) => setAction(String(value))}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {ACTIONS.map((option) => (
                    <SelectItem key={option} value={option}>
                      {option}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>
          <div className="space-y-2">
            <Label htmlFor="threshold">Threshold (fraction, e.g. 0.8)</Label>
            <Input id="threshold" name="threshold" type="number" step="0.01" defaultValue="0.8" required />
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-2">
              <Label htmlFor="creditLimit">Credit limit</Label>
              <Input id="creditLimit" name="creditLimit" type="number" placeholder="1000" />
            </div>
            <div className="space-y-2">
              <Label htmlFor="windowDays">Window (days)</Label>
              <Input id="windowDays" name="windowDays" type="number" placeholder="30" />
            </div>
          </div>
          <div className="space-y-2">
            <Label htmlFor="accountId">Engine account ID</Label>
            <Input id="accountId" name="accountId" placeholder="uuid of the account to watch" />
          </div>
          <DialogFooter>
            <Button type="submit" disabled={pending}>
              Create
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
