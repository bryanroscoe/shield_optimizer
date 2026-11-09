<script lang="ts">
  // A package id, setting key or component name with a line-break opportunity
  // after each separator, so "com.google.android.katniss:interactor" wraps as
  // "com.google.android.katniss:" / "interactor" rather than mid-word. `<wbr>`
  // adds no characters, so copying the text still gives the exact id. Hosts
  // keep `overflow-wrap: anywhere` as the fallback for a segment too long for
  // the cell on its own.
  let { text }: { text: string } = $props();

  const parts = $derived.by(() => {
    const out: string[] = [];
    let start = 0;
    for (let i = 0; i < text.length; i++) {
      if (".:/_@,".includes(text[i]) && i + 1 < text.length) {
        out.push(text.slice(start, i + 1));
        start = i + 1;
      }
    }
    out.push(text.slice(start));
    return out;
  });
</script>

{#each parts as part, i}{#if i > 0}<wbr />{/if}{part}{/each}
