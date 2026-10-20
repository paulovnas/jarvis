function rgb(hex: string) {
  const digits = hex.slice(1);
  const full = digits.length === 3 ? [...digits].map((digit) => digit + digit).join("") : digits;
  return [0, 2, 4].map((offset) => Number.parseInt(full.slice(offset, offset + 2), 16));
}

/** Change UI paint only; artwork and explicit visual inspection backdrops retain their colors. */
export function adoptTheme(css: string) {
  return css.replace(/([^{}]+)\{([^{}]*)\}/g, (block: string, selector: string, body: string) => {
    if (/checker|chroma|swatch|color-chip|preview-background|\.background-(light|dark)|onion|diff-overlay/.test(selector)) return block;
    const transformed = body.replace(/([-\w]+)\s*:\s*([^;{}]+);/g, (_declaration: string, property: string, value: string) => {
      if (!/^(color|background(?:-color)?|border(?:-(?:top|right|bottom|left))?(?:-color)?)$/.test(property)) return `${property}: ${value};`;
      const palette = value.replace(/#[0-9a-fA-F]{6}\b|#[0-9a-fA-F]{3}\b/g, (hex: string) => {
        const [r, g, b] = rgb(hex);
        const brightness = (r * 0.2126 + g * 0.7152 + b * 0.0722) / 255;
        const green = g - r > 25 && g > r * 1.12 && g > b * 1.08;
        const red = r > g * 1.3 && r > b * 1.2;
        const amber = r > b * 1.5 && g > b * 1.2;
        if (property === "color") return brightness < 0.55 ? green ? "#84d18a" : red ? "#ff9096" : amber ? "#edc46a" : "#ededed" : hex;
        if (property.startsWith("border")) return green ? "rgb(132 209 138 / 35%)" : red ? "rgb(255 144 150 / 35%)" : amber ? "rgb(237 196 106 / 35%)" : "rgb(255 255 255 / 12%)";
        return brightness > 0.55 ? green ? "#303e33" : red ? "#442d30" : amber ? "#443b28" : "#2b2d2e" : hex;
      }).replace(/rgba?\(\s*255\s*,\s*255\s*,\s*255\s*(?:,\s*([\d.]+))?\s*\)/g, (_color: string, alpha?: string) =>
        property.startsWith("background") ? `rgba(43,45,46,${alpha ?? 1})` : _color);
      return `${property}: ${palette};`;
    });
    const controls = selector.replace(/(^|[\s>+~,(])summary\b/g, '$1[data-slot="collapsible-trigger"]').replaceAll("[open]", "[data-open]");
    return `${controls}{${transformed}}`;
  });
}
