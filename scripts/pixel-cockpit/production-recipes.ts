export const productionRecipes = [
  { id: "neutral-anchor", title: "Referência neutra", kind: "source" },
  { id: "directional-anchor", title: "Referência por direção", kind: "source" },
  { id: "idle-breathing", title: "Repouso", kind: "animation" },
  { id: "walk-cycle", title: "Caminhada", kind: "animation" },
  { id: "basic-attack", title: "Ataque", kind: "animation" },
  { id: "hurt-reaction", title: "Reação ao dano", kind: "animation" },
  { id: "jump-hop", title: "Salto", kind: "animation" },
  { id: "death-downed", title: "Queda", kind: "animation" },
] as const;

const common = [
  "Use the approved source as the identity anchor: preserve silhouette, costume, palette, proportions and asymmetric accessories.",
  "Keep one coherent native pixel grid, crisp solid-color clusters and readable silhouettes; avoid antialiasing, blur, sub-pixel noise and mixed pixel sizes.",
  "Keep a fixed camera and shared body scale across the action. Preserve intentional compression, vertical travel and secondary motion; do not enlarge short poses to match the tallest frame.",
  "Separate the subject clearly from the requested flat background. Leave clearance for the complete body, weapons and effects; keep each pose and its detached parts visually attributable.",
  "Preserve the original reference and raw outputs. If postprocessing is needed, recover pixels and poses in derived files before packing; inspect the animation at runtime size.",
];

const phases: Record<string, string> = {
  "neutral-anchor": "Produce one neutral full-body identity reference. Use an upright rest pose appropriate to the anatomy, with no action-only effects or newly invented equipment. Keep approved permanent props and asymmetries. This is a static reference, not an animation sheet.",
  "directional-anchor": "Produce one static view in the requested direction. Keep handedness, costume asymmetries and equipment attached to their correct body sides. Do not mirror automatically. A turnaround of static views is not a walk cycle.",
  "idle-breathing": "Keep support contacts planted; use restrained breathing and secondary motion. The first-to-last transition must close naturally without shifting the body anchor.",
  "walk-cycle": "Distribute contact, down, passing and up phases across the configured frame count. Make the support groups alternate and the silhouettes distinct. Preserve coherent foot/paw/body contact and a seamless loop; do not substitute torso bobbing for locomotion.",
  "basic-attack": "Distribute anticipation, strike, impact and recovery across the configured frames. Keep the grip and weapon on the same body side, with readable spacing and complete follow-through. Do not cut the weapon when it crosses a nominal sheet cell.",
  "hurt-reaction": "Show a readable impact, recoil and recovery, preserving identity and support anatomy. Keep this distinct from an attack or defeated pose; preserve the configured one-shot behavior.",
  "jump-hop": "Show anticipation, takeoff, apex, descent and landing according to the configured frame count. Preserve intentional vertical displacement and body compression. Do not pin every airborne frame to a foot baseline or resize the crouch to standing height.",
  "death-downed": "Show support loss and settling into the configured final downed pose. Preserve the final hold and one-shot timing; do not force a loop or inflate low poses to standing height.",
};

export function productionRecipe(id: string, direction = "") {
  const recipe = productionRecipes.find((item) => item.id === id);
  if (!recipe) throw new Error("Receita de produção inexistente. Consulte as opções do catálogo.");
  if (id === "directional-anchor" && !/^[\p{L}\p{N} /_-]{1,80}$/u.test(direction.trim())) {
    throw new Error("Informe a direção da referência, por exemplo north, side ou northwest.");
  }
  return { ...recipe, version: 1, guidance: [...common, phases[id], ...(id === "directional-anchor" ? ["Requested direction: " + direction.trim() + "."] : [])].join("\n") };
}

interface CompiledRecipe {
  prompt: string;
  notes: string;
  sections: { id: string; lines: string[] }[];
}

/** Add discipline inside the original compiler sections, retaining its motion, QA and parameter contracts. */
export function enhanceMotionRecipe<T extends CompiledRecipe>(compiled: T, id: string, topology: string): T {
  const guidance = [...common, ...(phases[id] ? [phases[id]] : []), "Respect the configured " + topology + " anatomy and contact policy; never invent humanoid limbs for another topology."];
  const sections = compiled.sections.map((section) => section.id === "identity"
    ? { ...section, lines: [...new Set([...section.lines, ...guidance])] } : { ...section, lines: [...section.lines] });
  const marker = "Jarvis production discipline v1";
  return {
    ...compiled,
    sections,
    prompt: sections.map((section) => section.id.toUpperCase() + ": " + section.lines.join(" ")).join("\n"),
    notes: compiled.notes.includes(marker) ? compiled.notes : compiled.notes + "\n" + marker,
  };
}
