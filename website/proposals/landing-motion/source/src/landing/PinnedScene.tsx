import { useEffect, useRef, type ReactNode } from "react";
import { usePinnedProgress, useSimplified } from "./scroll";
import "./landing.css";

/**
 * Scène épinglée : une section haute (`height` en vh) contenant une scène de 100 svh qui reste collée
 * pendant que le scroll fait avancer la progression de 0 à 1.
 *
 * - `onProgress(p, simplified)` est appelé à chaque image. Écrire le style des éléments avec `setStyle`.
 * - `simplified` (mouvement réduit ou écran étroit) : la scène n'est plus épinglée, `onProgress(1)` est
 *   appelé une fois. Prévoir un état final lisible.
 * - `forcedProgress` fige la scène à une progression donnée (captures, tests, revue).
 * - Le contenu éditorial reste en HTML réel dans `children` (SEO, accessibilité).
 */
export function PinnedScene({
  height = 300,
  label,
  forcedProgress,
  onProgress,
  children,
}: {
  height?: number;
  /** Nom accessible de la scène. */
  label: string;
  forcedProgress?: number;
  onProgress: (p: number, simplified: boolean) => void;
  children: ReactNode;
}) {
  const ref = useRef<HTMLElement>(null);
  const simplified = useSimplified();
  const cb = useRef(onProgress);
  cb.current = onProgress;

  usePinnedProgress(ref, (p) => cb.current(p, false), simplified || forcedProgress !== undefined);

  useEffect(() => {
    if (forcedProgress !== undefined) cb.current(forcedProgress, false);
    else if (simplified) cb.current(1, true);
  }, [forcedProgress, simplified]);

  return (
    <section
      ref={ref}
      aria-label={label}
      className={simplified ? "fs-scene fs-scene--static" : "fs-scene"}
      style={simplified ? undefined : { height: `${height}vh` }}
    >
      <div className="fs-stage">{children}</div>
    </section>
  );
}

