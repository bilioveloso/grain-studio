/**
 * What to suggest on a fresh install, from what the computer actually is.
 *
 * The assistant drives the editor through tools, so a local model has to be one that calls tools reliably --
 * which rules out most small vision models. It does not have to see: the app measures the photo and hands over
 * numbers, and a circled area is already a mask, so a text model can do the work. Claude is what you choose when
 * you want eyes and judgement.
 */

export interface Hardware {
  ramGb: number;
  cpuCores: number;
  vramGb: number;
  gpuName: string;
  os: string;
}

export interface LocalModelChoice {
  /** What to pull, exactly as the model server names it. */
  model: string;
  /** Shown to the person, who should not have to read a tag. */
  label: string;
  downloadGb: number;
}

export type Suggestion = 'local' | 'claude' | 'none';

export interface SetupAdvice {
  suggested: Suggestion;
  /** The model the local option would pull here, or null when this computer should not run one. */
  local: LocalModelChoice | null;
  /** One sentence saying why, in the person's terms, naming what was found. */
  reason: string;
}

// Ollama tags, smallest first. Each is a tool-calling text model; sizes are the download, not the memory needed.
const MODELS: Array<LocalModelChoice & { needsGb: number }> = [
  { model: 'qwen3:1.7b', label: 'Qwen 3 (small)', downloadGb: 1.4, needsGb: 3 },
  { model: 'qwen3:4b', label: 'Qwen 3 (medium)', downloadGb: 2.6, needsGb: 6 },
  { model: 'qwen3:8b', label: 'Qwen 3 (large)', downloadGb: 5.2, needsGb: 10 },
];

/** Roughly what can be kept in memory: the card's own memory, or a share of system memory without one. */
export function usableMemoryGb(hardware: Hardware): number {
  if (hardware.vramGb >= 4) return hardware.vramGb;
  // On the processor a model has to share with the photo editor, which is not shy about memory either.
  return Math.max(0, Math.floor((hardware.ramGb - 8) / 2));
}

/** The largest model this computer can hold, or null when even the small one would crawl. */
export function modelFor(hardware: Hardware): LocalModelChoice | null {
  const memory = usableMemoryGb(hardware);
  const fits = MODELS.filter((m) => memory >= m.needsGb);
  if (!fits.length) return null;
  const best = fits[fits.length - 1];
  return { model: best.model, label: best.label, downloadGb: best.downloadGb };
}

/**
 * What this computer should be offered first. A capable machine is pointed at the local model, because it costs
 * nothing and needs no account; a thin one is pointed at Claude, because a model it cannot hold would be a poor
 * first impression; and either way the other choices stay on the screen.
 */
export function adviseSetup(hardware: Hardware): SetupAdvice {
  const local = modelFor(hardware);
  const card = hardware.gpuName?.trim();
  if (local && hardware.vramGb >= 6) {
    return {
      suggested: 'local',
      local,
      reason: `Your ${card || 'graphics card'} has ${hardware.vramGb} GB of its own memory, enough to run ${local.label} here for free.`,
    };
  }
  if (local) {
    return {
      suggested: 'local',
      local,
      reason: `This computer can run ${local.label} on its own, though it will think slowly with ${hardware.ramGb} GB of memory and no large graphics card.`,
    };
  }
  return {
    suggested: 'claude',
    local: null,
    reason: `With ${hardware.ramGb} GB of memory${
      hardware.vramGb ? ` and ${hardware.vramGb} GB on the graphics card` : ' and no dedicated graphics card'
    }, a model running here would be slow, so Claude is the better fit.`,
  };
}
