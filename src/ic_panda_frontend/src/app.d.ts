// See https://kit.svelte.dev/docs/types#app
// for information about these interfaces
declare global {
  namespace App {
    interface Error {
      code: string
      id: string
    }
    // interface Locals {}
    // interface PageData {}
    // interface Platform {}
  }
}

/* eslint-disable */

declare namespace svelteHTML {
  interface HTMLAttributes<T> {
    'on:pandaTriggerWallet'?: (event: CustomEvent<any>) => void
  }
}
