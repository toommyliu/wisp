const pending = new Map<string, (result: unknown) => void>();

type Host = Pick<typeof __host, "requestIo" | "cancelIo">;

export const requestIo = <A>(host: Host, request: unknown, signal?: AbortSignal, body?: Uint8Array): Promise<A> =>
  new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(signal.reason);
      return;
    }
    const submitted = JSON.parse(host.requestIo(JSON.stringify(request), body)) as
      | { id: string }
      | { result: A };
    if ("result" in submitted) {
      resolve(submitted.result);
      return;
    }
    const { id } = submitted;
    const cancel = () => {
      pending.delete(id);
      signal?.removeEventListener("abort", cancel);
      host.cancelIo(id);
      reject(signal?.reason);
    };
    pending.set(id, (result) => {
      pending.delete(id);
      signal?.removeEventListener("abort", cancel);
      resolve(result as A);
    });
    signal?.addEventListener("abort", cancel, { once: true });
  });

export const completeIo = (id: string, result: unknown): void => {
  pending.get(id)?.(result);
};
