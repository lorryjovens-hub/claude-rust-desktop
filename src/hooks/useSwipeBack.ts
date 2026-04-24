import { useCallback, useRef } from 'react';

interface SwipeBackOptions {
  threshold?: number;
  velocity?: number;
  onSwipeBack?: () => void;
  enabled?: boolean;
}

export function useSwipeBack({
  threshold = 80,
  velocity = 0.5,
  onSwipeBack,
  enabled = true,
}: SwipeBackOptions) {
  const startXRef = useRef(0);
  const startYRef = useRef(0);
  const startTimeRef = useRef(0);
  const isTrackingRef = useRef(false);

  const handleTouchStart = useCallback((e: React.TouchEvent) => {
    if (!enabled) return;
    const touch = e.touches[0];
    // Only track if starting from left edge (first 20px of screen)
    if (touch.clientX > 20) return;

    startXRef.current = touch.clientX;
    startYRef.current = touch.clientY;
    startTimeRef.current = Date.now();
    isTrackingRef.current = true;
  }, [enabled]);

  const handleTouchMove = useCallback((e: React.TouchEvent) => {
    if (!isTrackingRef.current || !enabled) return;
    const touch = e.touches[0];
    const dx = touch.clientX - startXRef.current;
    const dy = touch.clientY - startYRef.current;

    // If vertical movement is greater than horizontal, cancel tracking
    if (Math.abs(dy) > Math.abs(dx)) {
      isTrackingRef.current = false;
      return;
    }

    // If swiping right from left edge, prevent default to enable custom handling
    if (dx > 0) {
      // Optional: could add visual feedback here
    }
  }, [enabled]);

  const handleTouchEnd = useCallback((e: React.TouchEvent) => {
    if (!isTrackingRef.current || !enabled) return;
    isTrackingRef.current = false;

    const touch = e.changedTouches[0];
    const dx = touch.clientX - startXRef.current;
    const dy = touch.clientY - startYRef.current;
    const dt = Date.now() - startTimeRef.current;

    // Check if horizontal swipe is significant enough
    if (dx < threshold) return;
    if (Math.abs(dy) > Math.abs(dx) * 0.5) return; // Too much vertical movement

    const swipeVelocity = dx / dt;
    if (swipeVelocity < velocity && dx < threshold * 1.5) return;

    onSwipeBack?.();
  }, [enabled, threshold, velocity, onSwipeBack]);

  return {
    handlers: {
      onTouchStart: handleTouchStart,
      onTouchMove: handleTouchMove,
      onTouchEnd: handleTouchEnd,
    },
  };
}
