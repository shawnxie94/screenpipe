// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com
//
// S1 冻结语料脚本：从 dev 实例 (127.0.0.1:3130) 聚类开发会话并导出证据清单。
// 复现：SCREENPIPE_LOCAL_API_KEY=<key> bun freeze-sessions.ts
// 聚类规则：开发应用（cmux/ZCode）的 activity-intervals 按 gap≤15min 合并为会话，
// 保留时长 ≥10min 的会话作为冻结集（S1 要求 ≥10 个真实开发会话）。

const B = process.env.SCREENPIPE_LOCAL_API_URL ?? "http://127.0.0.1:3130";
const KEY = process.env.SCREENPIPE_LOCAL_API_KEY;
if (!KEY) throw new Error("SCREENPIPE_LOCAL_API_KEY 未设置");
const OUT = new URL(".", import.meta.url).pathname;

const headers = { Authorization: `Bearer ${KEY}`, "Content-Type": "application/json" };

const ivRes = await fetch(
  `${B}/activity-intervals?start_time=7d%20ago&end_time=now`, { headers },
);
const ivData = (await ivRes.json()) as { intervals: any[] };

const dev = ivData.intervals
  .filter((x) => /cmux|ZCode/i.test(x.app_name))
  .sort((a, b) => +new Date(a.start_at) - +new Date(b.start_at));

const sessions: any[] = [];
let cur: any = null;
for (const x of dev) {
  if (cur && (+new Date(x.start_at) - +new Date(cur.end_at)) / 60000 <= 15) {
    cur.end_at = x.end_at;
    cur.interval_ids.push(x.id);
    cur.evidence_hint += x.evidence_count;
  } else {
    cur = { start_at: x.start_at, end_at: x.end_at, interval_ids: [x.id], evidence_hint: x.evidence_count };
    sessions.push(cur);
  }
}

const frozen = sessions
  .map((s, i) => ({ ...s, mins: Math.round((+new Date(s.end_at) - +new Date(s.start_at)) / 60000), session: i + 1 }))
  .filter((s) => s.mins >= 10)
  .sort((a, b) => +new Date(a.start_at) - +new Date(b.start_at));

console.log(`候选会话 ${sessions.length}，冻结 ≥10min 的 ${frozen.length} 个`);

const manifest: any[] = [];
for (const s of frozen) {
  const evidences: any[] = [];
  for (const id of s.interval_ids) {
    const r = await fetch(`${B}/activity-intervals/${id}/evidence`, { headers });
    if (!r.ok) { console.error(`evidence ${id}: HTTP ${r.status}`); continue; }
    const d = (await r.json()) as any;
    for (const e of d.evidence ?? []) evidences.push({ interval_id: id, ...e });
  }
  const name = `session-${String(s.session).padStart(2, "0")}-${s.start_at.slice(0, 10)}`;
  await Bun.write(`${OUT}${name}-evidence.json`, JSON.stringify({
    session: name, start_at: s.start_at, end_at: s.end_at, duration_min: s.mins,
    interval_ids: s.interval_ids, evidence_count: evidences.length, evidence: evidences,
  }, null, 2));
  manifest.push({
    session: name, start_at: s.start_at, end_at: s.end_at, duration_min: s.mins,
    interval_ids: s.interval_ids, evidence_count: evidences.length,
  });
  console.log(`${name}: ${s.mins}m, intervals=${s.interval_ids.length}, evidence=${evidences.length}`);
}

await Bun.write(`${OUT}frozen-sessions.json`, JSON.stringify({
  frozen_at: new Date().toISOString(),
  source: `${B}/activity-intervals start_time=7d ago`,
  rule: "dev apps (cmux/ZCode), gap<=15min merge, >=10min kept",
  same_flow_note: "全部会话为 screenpipe 仓库开发流程（改需求→改代码→验证）的不同实例",
  sessions: manifest,
}, null, 2));
console.log(`冻结清单与证据已写入 ${OUT}`);
