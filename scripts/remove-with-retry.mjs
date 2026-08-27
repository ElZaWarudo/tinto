export async function removeWithRetry({ attempts, delay, remove }) {
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      remove();
      return;
    } catch (error) {
      const retryable =
        error?.code === "EPERM" || error?.code === "EBUSY" || error?.code === "ENOTEMPTY";
      if (!retryable || attempt === attempts) throw error;
      await delay();
    }
  }
}
