{{- define "hlin.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "hlin.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{- define "hlin.labels" -}}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{ include "hlin.selectorLabels" . }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{- define "hlin.selectorLabels" -}}
app.kubernetes.io/name: {{ include "hlin.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{- define "hlin.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "hlin.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Refusals, made at template time.

Both mirror something the shell refuses at startup, and they are here for the
same reason it does it there: the failure they prevent is silent. A shell that
accepts a forged header looks exactly like one that is working, and two replicas
signing with different keys fail one request in two rather than all of them.
*/}}
{{- define "hlin.validate" -}}
{{- if and (gt (int .Values.replicaCount) 1) (not .Values.signingKey.existingSecret) }}
{{- fail "replicaCount > 1 needs signingKey.existingSecret: each replica would otherwise generate its own key, and a token minted by one pod fails to verify against another's key set — intermittently, which is the worst way for it to fail." }}
{{- end }}
{{- if and (not .Values.postgres.enabled) (not .Values.config.databaseUrl) (not .Values.config.databaseUrlSecret.name) }}
{{- fail "no database: set postgres.enabled, config.databaseUrl, or config.databaseUrlSecret. A release build refuses to start without one, because the in-memory fallback loses every view anybody composes on the next restart — the thing Hlin is for, failing quietly." }}
{{- end }}
{{- $trust := 0 }}
{{- if .Values.config.caBundle.pem }}{{ $trust = add1 $trust }}{{ end }}
{{- if .Values.config.caBundle.existingConfigMap }}{{ $trust = add1 $trust }}{{ end }}
{{- if .Values.config.caBundle.existingSecret }}{{ $trust = add1 $trust }}{{ end }}
{{- if gt $trust 1 }}
{{- fail "config.caBundle: set at most one of pem, existingConfigMap, existingSecret. Only one can be mounted, so the others would be silently ignored — and a trust anchor that is silently ignored fails against every platform at once, looking like an outage." }}
{{- end }}
{{- if eq .Values.config.auth.strategy "anonymous" }}
{{- /* Nothing to demand. No provider, no proxy, no secret — and no
       acknowledgement, because there is nothing to accept: the shell refuses
       every write, so an open install cannot be vandalised by the anonymity
       that makes it reachable. */}}
{{- else if eq .Values.config.auth.strategy "trusted-header" }}
{{- if not .Values.config.auth.trustedHeader.acknowledgeProxyRequired }}
{{- fail "config.auth.trustedHeader.acknowledgeProxyRequired must be true: this strategy trusts whatever the header claims, so anything that can reach a pod directly is whoever it says it is. A pod IP is reachable from the rest of the namespace by default — an ingress in front is not on its own enough." }}
{{- end }}
{{- else if eq .Values.config.auth.strategy "oidc" }}
{{- if not .Values.config.auth.oidc.publicUrl }}
{{- fail "config.auth.oidc.publicUrl is required: the redirect URI is built from it and has to match what the provider was told, and the shell cannot see its own public address from inside the cluster." }}
{{- end }}
{{- if not .Values.config.auth.oidc.clientSecret.name }}
{{- fail "config.auth.oidc.clientSecret.name is required: the client secret comes from a Secret, never from values." }}
{{- end }}
{{- else }}
{{- fail (printf "config.auth.strategy must be anonymous, trusted-header or oidc, and is %q. `dev` is not offered: the shell refuses it outside a debug build, so a chart that rendered it would only produce a pod that crashes with an explanation." .Values.config.auth.strategy) }}
{{- end }}
{{- end }}

{{/*
Where the trust bundle comes from, if anywhere. Empty when none is configured,
which is what every template below tests.
*/}}
{{- define "hlin.trustMounted" -}}
{{- if or .Values.config.caBundle.pem .Values.config.caBundle.existingConfigMap .Values.config.caBundle.existingSecret }}yes{{ end }}
{{- end }}

{{- define "hlin.trustSource" -}}
{{- if .Values.config.caBundle.pem }}config.caBundle.pem
{{- else if .Values.config.caBundle.existingConfigMap }}ConfigMap {{ .Values.config.caBundle.existingConfigMap }}
{{- else }}Secret {{ .Values.config.caBundle.existingSecret }}{{ end }}
{{- end }}

{{/*
The filename under /etc/hlin-trust. An inline PEM is written under a name the
chart chooses; a borrowed one keeps its own key, so the mount and the setting
agree without the operator having to make them agree.
*/}}
{{- define "hlin.trustFile" -}}
{{- if .Values.config.caBundle.pem }}ca.pem{{ else }}{{ .Values.config.caBundle.key }}{{ end }}
{{- end }}
