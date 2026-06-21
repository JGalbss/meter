"use client"

import { ArrowRight, Lightning } from "@phosphor-icons/react"
import type { ReactNode } from "react"
import { useState, useTransition } from "react"
import { toast } from "sonner"

import { MeterMark } from "@/components/brand/logo"
import { CopyButton } from "@/components/copy-button"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  createAgentAction,
  createKeyAction,
  createOrgAction,
  finishOnboardingAction,
  runTestPingAction,
  type TestPing,
} from "@/app/onboarding/actions"

// Steps that show a progress pill (the hero and finale don't).
type Step = "org" | "agent" | "key" | "ping"

// Models guaranteed in the engine's hosted catalog, so the test ping always prices to a real debit.
const MODELS: readonly { value: string; label: string }[] = [
  { value: "claude-opus-4-8", label: "Claude Opus 4.8" },
  { value: "claude-sonnet-4-6", label: "Claude Sonnet 4.6" },
  { value: "gpt-5", label: "GPT-5" },
  { value: "gemini-2.5-pro", label: "Gemini 2.5 Pro" },
]

function formatCredits(value: string): string {
  const parsed = Number(value)
  if (Number.isNaN(parsed)) {
    return value
  }
  return parsed.toLocaleString(undefined, { maximumFractionDigits: 4 })
}

function stepsFor(hasOrg: boolean): readonly Step[] {
  if (hasOrg) {
    return ["agent", "key", "ping"]
  }
  return ["org", "agent", "key", "ping"]
}

const STEP_LABEL: Record<Step, string> = {
  org: "Workspace",
  agent: "Agent",
  key: "Key",
  ping: "Test ping",
}

export function OnboardingWizard({
  initialOrg,
}: {
  initialOrg: { id: string; name: string } | null
}) {
  const steps = stepsFor(initialOrg !== null)
  const [phase, setPhase] = useState<"hero" | Step | "done">("hero")
  const [orgId, setOrgId] = useState<string | null>(initialOrg?.id ?? null)
  const [orgName, setOrgName] = useState(initialOrg?.name ?? "")
  const [agentName, setAgentName] = useState("")
  const [keyName, setKeyName] = useState("")
  const [token, setToken] = useState<string | null>(null)
  const [model, setModel] = useState(MODELS[0].value)
  const [ping, setPing] = useState<TestPing | null>(null)
  const [pending, startTransition] = useTransition()

  const goTo = (next: "hero" | Step | "done") => setPhase(next)
  const firstStep = steps[0]
  const activeIndex = steps.indexOf(phase as Step)

  const onCreateOrg = () => {
    startTransition(async () => {
      const result = await createOrgAction({ name: orgName.trim() })
      if (!result.ok) {
        toast.error(result.error)
        return
      }
      setOrgId(result.orgId)
      setOrgName(result.orgName)
      goTo("agent")
    })
  }

  const onCreateAgent = () => {
    if (orgId === null) {
      return
    }
    startTransition(async () => {
      const result = await createAgentAction({
        orgId,
        name: agentName.trim(),
      })
      if (!result.ok) {
        toast.error(result.error)
        return
      }
      goTo("key")
    })
  }

  const onCreateKey = () => {
    if (orgId === null) {
      return
    }
    startTransition(async () => {
      const result = await createKeyAction({ orgId, name: keyName.trim() })
      if (!result.ok) {
        toast.error(result.error)
        return
      }
      setToken(result.token)
    })
  }

  const onRunPing = () => {
    if (orgId === null) {
      return
    }
    startTransition(async () => {
      const result = await runTestPingAction({ orgId, model })
      if (!result.ok) {
        toast.error(result.error)
        return
      }
      setPing(result.ping)
      goTo("done")
    })
  }

  const onFinish = () => {
    startTransition(async () => {
      await finishOnboardingAction()
    })
  }

  const onModelChange = (value: string | null) => {
    if (value !== null) {
      setModel(value)
    }
  }

  return (
    <div className="w-full max-w-xl">
      {phase !== "hero" && phase !== "done" && (
        <ol className="mb-8 flex items-center justify-center gap-2">
          {steps.map((step, index) => (
            <li key={step} className="flex items-center gap-2">
              <span
                data-active={index <= activeIndex}
                className="rounded-full px-3 py-1 text-xs font-medium tracking-wide text-muted-foreground transition-colors data-[active=true]:bg-primary data-[active=true]:text-primary-foreground"
              >
                {STEP_LABEL[step]}
              </span>
            </li>
          ))}
        </ol>
      )}

      <div key={phase} className="t-reveal">
        {phase === "hero" && <HeroStep onStart={() => goTo(firstStep)} />}

        {phase === "org" && (
          <StepShell
            title="Name your workspace"
            subtitle="An organization is the top of your hierarchy — teams, agents, and budgets live under it."
          >
            <div className="space-y-2">
              <Label htmlFor="org-name">Workspace name</Label>
              <Input
                id="org-name"
                value={orgName}
                onChange={(event) => setOrgName(event.target.value)}
                placeholder="Acme Inc"
                autoFocus
              />
            </div>
            <StepFooter
              onContinue={onCreateOrg}
              disabled={pending || orgName.trim().length === 0}
              label="Create workspace"
            />
          </StepShell>
        )}

        {phase === "agent" && (
          <StepShell
            title="Create your first agent"
            subtitle="An agent is the thing you meter — a chatbot, a copilot, a pipeline. You can add more later."
          >
            <div className="space-y-2">
              <Label htmlFor="agent-name">Agent name</Label>
              <Input
                id="agent-name"
                value={agentName}
                onChange={(event) => setAgentName(event.target.value)}
                placeholder="Support Copilot"
                autoFocus
              />
            </div>
            <StepFooter
              onContinue={onCreateAgent}
              disabled={pending || agentName.trim().length === 0}
              label="Create agent"
            />
          </StepShell>
        )}

        {phase === "key" && (
          <StepShell
            title="Mint an API key"
            subtitle="Your SDK uses this to report usage. It's shown once — copy it now."
          >
            {token === null && (
              <>
                <div className="space-y-2">
                  <Label htmlFor="key-name">Key name</Label>
                  <Input
                    id="key-name"
                    value={keyName}
                    onChange={(event) => setKeyName(event.target.value)}
                    placeholder="production"
                    autoFocus
                  />
                </div>
                <StepFooter
                  onContinue={onCreateKey}
                  disabled={pending || keyName.trim().length === 0}
                  label="Mint key"
                />
              </>
            )}
            {token !== null && (
              <div className="space-y-4">
                <div className="space-y-2">
                  <Label htmlFor="key-token">Your API key</Label>
                  <div className="flex items-center gap-2">
                    <Input
                      id="key-token"
                      readOnly
                      value={token}
                      className="font-mono text-xs"
                    />
                    <CopyButton value={token} />
                  </div>
                </div>
                <div className="flex justify-end">
                  <Button onClick={() => goTo("ping")} disabled={pending}>
                    I&apos;ve saved it
                    <ArrowRight />
                  </Button>
                </div>
              </div>
            )}
          </StepShell>
        )}

        {phase === "ping" && (
          <StepShell
            title="Send a test event"
            subtitle="We'll fund a demo account, meter one real event through the engine, and watch the credits burn — the whole loop, end to end."
          >
            <div className="space-y-2">
              <Label htmlFor="ping-model">Model</Label>
              <Select value={model} onValueChange={onModelChange}>
                <SelectTrigger id="ping-model">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {MODELS.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="flex justify-end">
              <Button onClick={onRunPing} disabled={pending}>
                <Lightning weight="fill" />
                {pending ? "Metering…" : "Send test event"}
              </Button>
            </div>
          </StepShell>
        )}

        {phase === "done" && ping !== null && (
          <DoneStep ping={ping} onFinish={onFinish} pending={pending} />
        )}
      </div>
    </div>
  )
}

function HeroStep({ onStart }: { onStart: () => void }) {
  return (
    <div className="flex flex-col items-center text-center">
      <span className="mb-6 flex size-16 items-center justify-center rounded-2xl bg-primary text-primary-foreground shadow-sm">
        <MeterMark size={36} />
      </span>
      {/* transitions.dev texts-reveal (mount variant). */}
      <div className="t-stagger-reveal">
        <h1 className="t-stagger-line t-stagger-line--1 font-heading text-3xl font-semibold tracking-tight">
          Let&apos;s measure your agents
        </h1>
        <p className="t-stagger-line t-stagger-line--2 mt-2 max-w-sm text-sm text-muted-foreground">
          Four steps to your first metered event: a workspace, an agent, a key,
          and a live test ping.
        </p>
      </div>
      <Button size="lg" className="mt-8" onClick={onStart}>
        Get started
        <ArrowRight />
      </Button>
    </div>
  )
}

function StepShell({
  title,
  subtitle,
  children,
}: {
  title: string
  subtitle: string
  children: ReactNode
}) {
  return (
    <div className="rounded-xl border bg-card p-8 shadow-sm">
      <div className="t-stagger-reveal mb-6">
        <h2 className="t-stagger-line t-stagger-line--1 font-heading text-xl font-semibold tracking-tight">
          {title}
        </h2>
        <p className="t-stagger-line t-stagger-line--2 mt-1 text-sm text-muted-foreground">
          {subtitle}
        </p>
      </div>
      <div className="space-y-6">{children}</div>
    </div>
  )
}

function StepFooter({
  onContinue,
  disabled,
  label,
}: {
  onContinue: () => void
  disabled: boolean
  label: string
}) {
  return (
    <div className="flex justify-end">
      <Button onClick={onContinue} disabled={disabled}>
        {label}
        <ArrowRight />
      </Button>
    </div>
  )
}

function DoneStep({
  ping,
  onFinish,
  pending,
}: {
  ping: TestPing
  onFinish: () => void
  pending: boolean
}) {
  return (
    <div className="flex flex-col items-center text-center">
      {/* transitions.dev success-check: fade + rotate + bob + stroke-draw. */}
      <span className="t-success-check mb-6 text-primary" data-state="in">
        <svg viewBox="0 0 48 48" fill="none" width={64} height={64}>
          <path
            d="M14 25 l7 7 l13 -16"
            stroke="currentColor"
            strokeWidth={4}
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </span>
      <div className="t-stagger-reveal">
        <h2 className="t-stagger-line t-stagger-line--1 font-heading text-2xl font-semibold tracking-tight">
          You metered your first event
        </h2>
        <p className="t-stagger-line t-stagger-line--2 mt-1 text-sm text-muted-foreground">
          One {ping.model} call priced through the engine and burned down the
          ledger — exactly what your agents will do in production.
        </p>
      </div>

      <div className="mt-8 grid w-full grid-cols-3 gap-3">
        <Stat label="Granted" value={formatCredits(ping.granted)} />
        <Stat label="Burned" value={`−${formatCredits(ping.credits)}`} accent />
        <Stat label="Remaining" value={formatCredits(ping.balanceAfter)} />
      </div>

      <Button size="lg" className="mt-8" onClick={onFinish} disabled={pending}>
        Open the dashboard
        <ArrowRight />
      </Button>
    </div>
  )
}

function Stat({
  label,
  value,
  accent,
}: {
  label: string
  value: string
  accent?: boolean
}) {
  return (
    <div className="rounded-lg border bg-card p-4">
      <p className="text-xs text-muted-foreground">{label}</p>
      <p
        data-accent={accent === true}
        className="mt-1 font-heading text-2xl font-semibold tabular-nums data-[accent=true]:text-primary"
      >
        {value}
      </p>
    </div>
  )
}
