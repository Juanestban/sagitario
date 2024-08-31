interface BuildParams {
  flags?: unknown;
}

export async function build({ flags }: BuildParams) {
  console.log(flags);
}
