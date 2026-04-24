import { useCallback, useRef, useState } from 'react';

interface LongPressOptions {
  threshold?: number;
  onLongPress: (e: React.TouchEvent | React.MouseEvent) => void;
  onClick?: (e: React.TouchEvent | React.MouseEvent) => void;
  onTouchStart?: (e: React.TouchEvent) => void;
  onTouchEnd?: (e: React.TouchEvent) => void;
}

export function useLongPress({
  threshold = 500,
  onLongPress,
  onClick,
  onTouchStart,
  onTouchEnd,
}: LongPressOptions) {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const startPosRef = useRef<{ x: number; y: number } | null>(null);
  const isLongPressRef = useRef(false);
  const [isPressing, setIsPressing] = useState(false);

  const clearTimer = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const handleTouchStart = useCallback((e: React.TouchEvent) => {
    const touch = e.touches[0];
    startPosRef.current = { x: touch.clientX, y: touch.clientY };
    isLongPressRef.current = false;
    setIsPressing(true);

    if (onTouchStart) onTouchStart(e);

    timerRef.current = setTimeout(() => {
      isLongPressRef.current = true;
      setIsPressing(false);
      onLongPress(e);
    }, threshold);
  }, [threshold, onLongPress, onTouchStart]);

  const handleTouchMove = useCallback((e: React.TouchEvent) => {
    if (!startPosRef.current) return;
    const touch = e.touches[0];
    const dx = Math.abs(touch.clientX - startPosRef.current.x);
    const dy = Math.abs(touch.clientY - startPosRef.current.y);
    // If moved more than 10px, cancel long press
    if (dx > 10 || dy > 10) {
      clearTimer();
      setIsPressing(false);
      startPosRef.current = null;
    }
  }, [clearTimer]);

  const handleTouchEnd = useCallback((e: React.TouchEvent) => {
    clearTimer();
    setIsPressing(false);
    startPosRef.current = null;

    if (onTouchEnd) onTouchEnd(e);

    // If it wasn't a long press, treat as click
    if (!isLongPressRef.current && onClick) {
      onClick(e);
    }
    isLongPressRef.current = false;
  }, [clearTimer, onClick, onTouchEnd]);

  const handleTouchCancel = useCallback(() => {
    clearTimer();
    setIsPressing(false);
    startPosRef.current = null;
    isLongPressRef.current = false;
  }, [clearTimer]);

  // Mouse events for desktop fallback
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    startPosRef.current = { x: e.clientX, y: e.clientY };
    isLongPressRef.current = false;
    setIsPressing(true);

    timerRef.current = setTimeout(() => {
      isLongPressRef.current = true;
      setIsPressing(false);
      onLongPress(e);
    }, threshold);
  }, [threshold, onLongPress]);

  const handleMouseUp = useCallback((e: React.MouseEvent) => {
    clearTimer();
    setIsPressing(false);
    startPosRef.current = null;

    if (!isLongPressRef.current && onClick) {
      onClick(e);
    }
    isLongPressRef.current = false;
  }, [clearTimer, onClick]);

  const handleMouseLeave = useCallback(() => {
    clearTimer();
    setIsPressing(false);
    startPosRef.current = null;
    isLongPressRef.current = false;
  }, [clearTimer]);

  return {
    handlers: {
      onTouchStart: handleTouchStart,
      onTouchMove: handleTouchMove,
      onTouchEnd: handleTouchEnd,
      onTouchCancel: handleTouchCancel,
      onMouseDown: handleMouseDown,
      onMouseUp: handleMouseUp,
      onMouseLeave: handleMouseLeave,
    },
    isPressing,
  };
}
