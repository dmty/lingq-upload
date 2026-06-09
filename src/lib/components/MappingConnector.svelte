<script lang="ts">
  /**
   * One SVG `<line>` between the chapter row and the track row of a paired
   * chapter↔track entry. Stroke colour reflects confidence; geometry is
   * computed from row bounding rects supplied by the parent.
   */
  type Props = {
    x1: number;
    y1: number;
    x2: number;
    y2: number;
    confidence: number;
  };
  const { x1, y1, x2, y2, confidence }: Props = $props();

  function stroke(c: number): string {
    if (c >= 0.8) return "#16a34a"; // green
    if (c >= 0.6) return "#d97706"; // amber
    return "#dc2626"; // red
  }
</script>

<line
  {x1}
  {y1}
  {x2}
  {y2}
  stroke={stroke(confidence)}
  stroke-width="2"
  stroke-linecap="round"
  data-testid="mapping-connector"
  data-confidence={confidence}
/>
