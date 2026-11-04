import { ChangeDetectionStrategy, Component } from "@angular/core";
import { Overview } from "./overview/overview";

@Component({
  selector: "app-root",
  imports: [Overview],
  template: "<app-overview />",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class App {}
