import { DOCUMENT } from "@angular/common";
import {
  DestroyRef,
  EnvironmentProviders,
  inject,
  makeEnvironmentProviders,
  provideEnvironmentInitializer,
} from "@angular/core";
import { HxTheme, provideHxTheme } from "@gravionlabs/helix-ui";

const DARK_QUERY = "(prefers-color-scheme: dark)";

/**
 * The helix light and dark theme, following the colour scheme of the OS: dark at start-up when the
 * system is dark (HxTheme reads it once) and switched whenever the system setting changes. Nothing
 * is remembered, so the OS stays the only source.
 */
export function provideSystemTheme(): EnvironmentProviders {
  return makeEnvironmentProviders([
    provideHxTheme(),
    provideEnvironmentInitializer(() => {
      const theme = inject(HxTheme);
      const media = inject(DOCUMENT).defaultView?.matchMedia?.(DARK_QUERY);
      if (!media) return;
      const follow = (event: MediaQueryListEvent) => theme.setDark(event.matches);
      media.addEventListener("change", follow);
      inject(DestroyRef).onDestroy(() => media.removeEventListener("change", follow));
    }),
  ]);
}
