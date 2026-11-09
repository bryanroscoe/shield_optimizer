import { mount } from "svelte";
import AppReportSheet from "../../src/components/AppReportSheet.svelte";

export function mountReport(target: HTMLElement, props: Parameters<typeof AppReportSheet>[1]) {
  return mount(AppReportSheet, { target, props });
}
