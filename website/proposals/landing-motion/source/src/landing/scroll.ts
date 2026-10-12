import { useEffect, useRef, useSyncExternalStore, type RefObject } from "react";

/** Outils de progression du scroll. Aucune dépendance. Le scroll est l'horloge : retour arrière gratuit. */

export const clamp = (v: number, min = 0, max = 1) => Math.min(max, Math.max(min, v));

/** Progression 0 à 1 de `p` entre `from` et `to`. Hors de l'intervalle : 0 ou 1. */
export const segment = (p: number, from: number, to: number) => clamp((p - from) / (to - from));

/** Adoucit une progression (départ et arrivée en douceur). */
export const smooth = (t: number) => t * t * (3 - 2 * t);

export const lerp = (a: number, b: number, t: number) => a + (b - a) * t;

/** Écrit directement le style d'un élément : pas de rendu React par image. */
export function setStyle(
  el: HTMLElement | null,
  s: { opacity?: number; x?: number; y?: number; scale?: number; rotate?: number; rotateX?: number; blur?: number },
) {
  if (!el) return;
  const { opacity, x = 0, y = 0, scale = 1, rotate = 0, rotateX = 0, blur } = s;
  el.style.transform = `translate3d(${x}px, ${y}px, 0) rotate(${rotate}deg) rotateX(${rotateX}deg) scale(${scale})`;
  if (opacity !== undefined) el.style.opacity = String(opacity);
  if (blur !== undefined) el.style.filter = blur > 0 ? `blur(${blur}px)` : "none";
}

const QUERY = "(prefers-reduced-motion: reduce), (max-width: 760px)";

/** Vrai si l'utilisateur réduit les animations ou si l'écran est étroit. Réagit aux changements. */
export function useSimplified(): boolean {
  return useSyncExternalStore(
    (notify) => {
      const m = window.matchMedia(QUERY);
      m.addEventListener("change", notify);
      return () => m.removeEventListener("change", notify);
    },
    () => window.matchMedia(QUERY).matches,
    () => false,
  );
}

/**
 * Calcule la progression 0 à 1 d'une scène épinglée (section haute, scène collante à l'intérieur)
 * et l'envoie à `onProgress`. Un seul écouteur de scroll, coalescé par requestAnimationFrame,
 * nettoyé au démontage. Désactivé (`disabled`) : n'écoute rien.
 */
export function usePinnedProgress(
  ref: RefObject<HTMLElement | null>,
  onProgress: (p: number) => void,
  disabled = false,
) {
  const cb = useRef(onProgress);
  cb.current = onProgress;

  useEffect(() => {
    const el = ref.current;
    if (!el || disabled) return;
    let frame = 0;
    const update = () => {
      frame = 0;
      const b = el.getBoundingClientRect();
      const length = b.height - window.innerHeight;
      cb.current(length > 0 ? clamp(-b.top / length) : 0);
    };
    const request = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    window.addEventListener("scroll", request, { passive: true });
    window.addEventListener("resize", request);
    request();
    return () => {
      if (frame) cancelAnimationFrame(frame);
      window.removeEventListener("scroll", request);
      window.removeEventListener("resize", request);
    };
  }, [ref, disabled]);
}

