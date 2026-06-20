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
import type { ApiKeyRole } from "@/lib/meter/types";
import { createApiKeyAction } from "./actions";

const ROLES: readonly { value: ApiKeyRole; label: string }[] = [
  { value: "viewer", label: "Viewer — read-only" },
  { value: "member", label: "Member — read & write" },
  { value: "admin", label: "Admin — full access" },
];

export function CreateApiKeyDialog({ orgId }: { orgId: string }) {
  const [open, setOpen] = useState(false);
  const [pending, startTransition] = useTransition();
  const [token, setToken] = useState<string | null>(null);
  const [role, setRole] = useState<ApiKeyRole>("member");

  const onOpenChange = (next: boolean) => {
    setOpen(next);
    if (!next) {
      setToken(null);
      setRole("member");
    }
  };

  // The select is typed over our role values; it emits ApiKeyRole | null (null only on clear).
  const onRoleChange = (value: ApiKeyRole | null) => {
    if (value !== null) {
      setRole(value);
    }
  };

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const name = String(data.get("name") ?? "");
    startTransition(async () => {
      const result = await createApiKeyAction({ orgId, name, role });
      if (!result.ok) {
        toast.error(result.error);
        return;
      }
      setToken(result.token);
      toast.success("API key created");
    });
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogTrigger render={<Button size="sm" />}>New API key</DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>New API key</DialogTitle>
          <DialogDescription>The token is shown once — copy it now.</DialogDescription>
        </DialogHeader>
        {token === null && (
          <form onSubmit={submit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="name">Name</Label>
              <Input id="name" name="name" placeholder="ci pipeline" required />
            </div>
            <div className="space-y-2">
              <Label htmlFor="role">Role</Label>
              <Select value={role} onValueChange={onRoleChange}>
                <SelectTrigger id="role">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {ROLES.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <DialogFooter>
              <Button type="submit" disabled={pending}>
                Create
              </Button>
            </DialogFooter>
          </form>
        )}
        {token !== null && (
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="token">Token</Label>
              <Input id="token" readOnly value={token} className="font-mono text-xs" />
            </div>
            <DialogFooter>
              <Button onClick={() => onOpenChange(false)}>Done</Button>
            </DialogFooter>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
