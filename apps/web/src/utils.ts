import { useEffect, useState } from "react";

/** Returns `false` until `ms` milliseconds have elapsed since mount. */
export function useMinimumDuration(ms: number): boolean {
  const [elapsed, setElapsed] = useState(false);

  useEffect(() => {
    const timer = setTimeout(() => setElapsed(true), ms);
    return () => clearTimeout(timer);
  }, [ms]);

  return elapsed;
}
