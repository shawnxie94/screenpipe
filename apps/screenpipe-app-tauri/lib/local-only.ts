// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

/**
 * Self-hosted builds exclude Screenpipe-operated services. This deliberately
 * does not restrict user-configured third-party AI APIs or local endpoints.
 */
export const IS_LOCAL_ONLY_BUILD =
  process.env.NEXT_PUBLIC_SCREENPIPE_LOCAL_ONLY === "true";
