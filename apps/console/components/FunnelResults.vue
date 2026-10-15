<script setup lang="ts">
import type { ProViewFunnelPreviewStep } from "~/types/pro-view";
const props = defineProps<{
  label: string;
  steps: ProViewFunnelPreviewStep[];
  sameSession: boolean;
}>();
const cohortLabel = computed(() =>
  props.sameSession ? "estimated sessions" : "estimated visitors",
);
const total = computed(() => props.steps.at(-1));
const percent = (value: number) =>
  `${value.toLocaleString(undefined, { maximumFractionDigits: 1 })}%`;
function duration(value?: number | null) {
  if (value == null || !Number.isFinite(value)) return "Unavailable";
  if (value < 1000) return `${Math.round(value)} ms`;
  if (value < 60000) return `${Math.round(value / 1000)}s`;
  if (value < 3600000)
    return `${(value / 60000).toLocaleString(undefined, { maximumFractionDigits: 1 })}m`;
  return `${(value / 3600000).toLocaleString(undefined, { maximumFractionDigits: 1 })}h`;
}
</script>

<template>
  <section class="funnel-result" :aria-label="`${label} funnel results`">
    <header class="funnel-summary">
      <div>
        <small>{{ label }}</small>
        <h3>From first step to completion</h3>
      </div>
      <div>
        <strong>{{
          steps[0]?.count ? percent(total?.overall_conversion ?? 0) : "—"
        }}</strong
        ><span>Total conversion</span>
      </div>
      <div>
        <strong>{{ total?.count.toLocaleString() ?? "—" }}</strong
        ><span>{{ cohortLabel }} completed</span>
      </div>
    </header>
    <ol class="result-steps">
      <li v-for="(step, index) in steps" :key="step.id">
        <span class="result-number" aria-hidden="true">{{ index + 1 }}</span>
        <div class="result-body">
          <div class="result-step-heading">
            <h4>{{ step.name }}</h4>
            <strong
              >{{ step.count.toLocaleString() }}
              <small>{{ cohortLabel }}</small></strong
            >
          </div>
          <div class="result-track" aria-hidden="true">
            <span
              :style="{
                width: `${Math.max(0, Math.min(100, step.overall_conversion))}%`,
              }"
            ></span>
          </div>
          <div class="result-detail">
            <span>{{
              index === 0
                ? "Entry population"
                : `${step.dropoff_count.toLocaleString()} lost from previous step · ${percent(step.dropoff_rate)}`
            }}</span
            ><span>{{ percent(step.overall_conversion) }} of entries</span>
          </div>
          <p v-if="index < steps.length - 1" class="result-timing">
            To next step: {{ duration(step.median_time_to_next_ms) }} median ·
            {{ duration(step.avg_time_to_next_ms) }} average
          </p>
        </div>
      </li>
    </ol>
    <details class="result-table">
      <summary>View accessible results table</summary>
      <div
        class="result-table-scroll"
        tabindex="0"
        role="region"
        :aria-label="`${label} funnel table`"
      >
        <table>
          <caption>
            {{
              label
            }}
            ·
            {{
              cohortLabel
            }}
          </caption>
          <thead>
            <tr>
              <th scope="col">Step</th>
              <th scope="col">Reached</th>
              <th scope="col">From previous</th>
              <th scope="col">Overall</th>
              <th scope="col">Lost</th>
              <th scope="col">Loss rate</th>
              <th scope="col">Median to next</th>
              <th scope="col">Average to next</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(step, index) in steps" :key="step.id">
              <th scope="row">{{ index + 1 }}. {{ step.name }}</th>
              <td>{{ step.count.toLocaleString() }}</td>
              <td>{{ index ? percent(step.step_conversion) : "—" }}</td>
              <td>{{ percent(step.overall_conversion) }}</td>
              <td>{{ index ? step.dropoff_count.toLocaleString() : "—" }}</td>
              <td>{{ index ? percent(step.dropoff_rate) : "—" }}</td>
              <td>{{ duration(step.median_time_to_next_ms) }}</td>
              <td>{{ duration(step.avg_time_to_next_ms) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </details>
  </section>
</template>

<style scoped>
.funnel-result {
  color: #f7f7f2;
  border: 1px solid #34363d;
  border-radius: 14px;
  background: #18191dcc;
  padding: clamp(12px, 2vw, 20px);
  margin-bottom: 16px;
  min-width: 0;
}
.funnel-summary,
.result-step-heading,
.result-detail {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}
.funnel-summary {
  padding-bottom: 18px;
  border-bottom: 1px solid #34363d;
}
.funnel-summary > div {
  display: grid;
  gap: 4px;
}
.funnel-summary small,
.result-timing,
.result-detail,
.result-step-heading small {
  color: #b6b7bb;
  font-size: 0.73rem;
}
h3,
h4 {
  color: #f7f7f2;
  margin: 0;
  overflow-wrap: anywhere;
}
h3 {
  font-size: 0.9rem;
}
h4 {
  font-size: 0.86rem;
}
.funnel-summary strong {
  color: #d8ff52;
  font-size: 1.6rem;
  font-variant-numeric: tabular-nums;
}
.funnel-summary span {
  font-size: 0.72rem;
}
.result-steps {
  list-style: none;
  padding: 0;
  margin: 20px 0;
  display: grid;
  gap: 20px;
}
.result-steps li {
  display: flex;
  gap: 12px;
}
.result-number {
  width: 28px;
  height: 28px;
  border: 1px solid #636773;
  border-radius: 50%;
  display: grid;
  place-items: center;
  flex: 0 0 auto;
  color: #d8ff52;
  font-family: var(--font-mono);
  font-size: 0.75rem;
}
.result-body {
  flex: 1;
  min-width: 0;
}
.result-step-heading strong {
  color: #f7f7f2;
  font-size: 1rem;
  font-variant-numeric: tabular-nums;
}
.result-step-heading small {
  font-weight: 400;
}
.result-track {
  height: 10px;
  background: #34363d;
  border-radius: 999px;
  margin: 10px 0;
  overflow: hidden;
}
.result-track span {
  display: block;
  height: 100%;
  background: linear-gradient(90deg, #3d5afe, #90a0ff);
  border-radius: inherit;
}
.result-timing {
  margin: 6px 0 0;
}
.result-table summary {
  cursor: pointer;
  color: #d8ff52;
  font-size: 0.8rem;
  padding: 10px 0;
}
.result-table-scroll {
  overflow-x: auto;
}
table {
  width: 100%;
  min-width: 680px;
  border-collapse: collapse;
  font-size: 0.72rem;
  font-variant-numeric: tabular-nums;
}
caption {
  text-align: left;
  padding: 10px 0;
}
th,
td {
  text-align: right;
  padding: 9px;
  border-top: 1px solid #34363d;
}
th:first-child {
  text-align: left;
}
th {
  color: #f7f7f2;
}
@media (max-width: 600px) {
  .funnel-summary > div:first-child {
    flex-basis: 100%;
  }
  .result-step-heading {
    align-items: flex-start;
    flex-direction: column;
    gap: 5px;
  }
  .result-detail {
    align-items: flex-start;
    flex-direction: column;
    gap: 4px;
  }
}
</style>
