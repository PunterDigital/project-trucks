import { useEffect, useState } from "react";

/**
 * Return a value that lags behind the input by `delay` ms. Useful for
 * smoothing out fast typing before kicking off a fetch.
 */
export function useDebounce<T>(value: T, delay = 300): T {
  const [debounced, setDebounced] = useState(value);

  useEffect(() => {
    const t = setTimeout(() => setDebounced(value), delay);
    return () => clearTimeout(t);
  }, [value, delay]);

  return debounced;
}
