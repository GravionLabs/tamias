import { Directive, ElementRef, EventEmitter, OnDestroy, Output, inject } from "@angular/core";

export interface ChartSize {
  width: number;
  height: number;
}

/** Reports the content size of its element whenever it changes (window resize, layout). */
@Directive({ selector: "[appChartBox]" })
export class ChartBox implements OnDestroy {
  // Decorator form: the specs compile templates without the Angular compiler transform.
  @Output() readonly sized = new EventEmitter<ChartSize>();
  private readonly observer =
    typeof ResizeObserver === "undefined"
      ? null
      : new ResizeObserver((entries) => {
          const rect = entries[entries.length - 1]?.contentRect;
          if (rect && rect.width > 0 && rect.height > 0) {
            this.sized.emit({ width: rect.width, height: rect.height });
          }
        });

  constructor() {
    this.observer?.observe(inject<ElementRef<Element>>(ElementRef).nativeElement);
  }

  ngOnDestroy(): void {
    this.observer?.disconnect();
  }
}
