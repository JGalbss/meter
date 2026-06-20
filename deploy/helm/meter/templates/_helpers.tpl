{{/* Chart name, optionally overridden. */}}
{{- define "meter.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/* Fully qualified app name. */}}
{{- define "meter.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name (include "meter.name" .) | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{/* Common labels. */}}
{{- define "meter.labels" -}}
app.kubernetes.io/name: {{ include "meter.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" }}
{{- end -}}

{{/* Selector labels for a named component. */}}
{{- define "meter.selectorLabels" -}}
app.kubernetes.io/name: {{ include "meter.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/component: {{ .component }}
{{- end -}}

{{/* The Secret holding database credentials. */}}
{{- define "meter.secretName" -}}
{{- printf "%s-credentials" (include "meter.fullname" .) -}}
{{- end -}}

{{/* In-cluster Postgres host (the postgres Service). */}}
{{- define "meter.postgresHost" -}}
{{- printf "%s-postgres" (include "meter.fullname" .) -}}
{{- end -}}

{{/* In-cluster ClickHouse host (the clickhouse Service). */}}
{{- define "meter.clickhouseHost" -}}
{{- printf "%s-clickhouse" (include "meter.fullname" .) -}}
{{- end -}}

{{/* Postgres connection URL: explicit override, else the in-cluster service. */}}
{{- define "meter.databaseUrl" -}}
{{- if .Values.engine.databaseUrl -}}
{{- .Values.engine.databaseUrl -}}
{{- else -}}
{{- printf "postgres://%s:%s@%s:%d/%s" .Values.credentials.postgresUser .Values.credentials.postgresPassword (include "meter.postgresHost" .) (int .Values.postgres.port) .Values.credentials.postgresDatabase -}}
{{- end -}}
{{- end -}}

{{/* Control-plane Postgres URL: its own override, else the same database the engine uses (they share
     one Postgres), so disabling the in-cluster Postgres only needs `engine.databaseUrl`. */}}
{{- define "meter.controlPlaneDatabaseUrl" -}}
{{- if .Values.controlPlane.databaseUrl -}}
{{- .Values.controlPlane.databaseUrl -}}
{{- else -}}
{{- include "meter.databaseUrl" . -}}
{{- end -}}
{{- end -}}

{{/* ClickHouse HTTP URL: explicit override, else the in-cluster service. */}}
{{- define "meter.clickhouseUrl" -}}
{{- if .Values.engine.clickhouseUrl -}}
{{- .Values.engine.clickhouseUrl -}}
{{- else -}}
{{- printf "http://%s:%d" (include "meter.clickhouseHost" .) (int .Values.clickhouse.httpPort) -}}
{{- end -}}
{{- end -}}

{{/* Engine internal URL the control plane calls. */}}
{{- define "meter.engineUrl" -}}
{{- printf "http://%s-engine:%d" (include "meter.fullname" .) (int .Values.engine.listenPort) -}}
{{- end -}}

{{/* Control-plane internal URL the dashboard calls. */}}
{{- define "meter.controlPlaneUrl" -}}
{{- printf "http://%s-control-plane:%d" (include "meter.fullname" .) (int .Values.controlPlane.port) -}}
{{- end -}}
