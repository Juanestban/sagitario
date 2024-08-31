interface PreviewParams {
  flags?: unknown;
}

export async function preview({ flags }: PreviewParams) {
  async function closed() {}

  return { closed };
}
