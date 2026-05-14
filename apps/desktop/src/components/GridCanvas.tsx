import { useEffect, useRef } from 'react';

/**
 * Placeholder grid canvas. Phase 0 just draws a static demo grid so the
 * window has something visible; Phase 1 replaces this with the real
 * lattice-driven renderer (see PLAN.md §8.2).
 */
export function GridCanvas() {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const draw = () => {
      const dpr = window.devicePixelRatio || 1;
      const w = canvas.clientWidth;
      const h = canvas.clientHeight;
      canvas.width = Math.floor(w * dpr);
      canvas.height = Math.floor(h * dpr);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);

      ctx.fillStyle = '#23262b';
      ctx.fillRect(0, 0, w, h);

      const cell = 48;
      ctx.strokeStyle = '#33373d';
      ctx.lineWidth = 1;
      ctx.beginPath();
      for (let x = 0; x < w; x += cell) {
        ctx.moveTo(x + 0.5, 0);
        ctx.lineTo(x + 0.5, h);
      }
      for (let y = 0; y < h; y += cell) {
        ctx.moveTo(0, y + 0.5);
        ctx.lineTo(w, y + 0.5);
      }
      ctx.stroke();

      ctx.fillStyle = '#7a8593';
      ctx.font = '12px system-ui, sans-serif';
      ctx.fillText('Phase 0 placeholder — real lattice renderer lands in Phase 1', 12, 24);
    };

    draw();
    const ro = new ResizeObserver(draw);
    ro.observe(canvas);
    return () => ro.disconnect();
  }, []);

  return <canvas ref={canvasRef} style={{ width: '100%', height: '100%', display: 'block' }} />;
}
