import { Panel } from '../components/ui/AppProperties';
import { useEditorStore } from '../store/useEditorStore';
import { useUIStore } from '../store/useUIStore';

/**
 * Hand a request to the Claude panel from anywhere in the app.
 *
 * The panel only exists inside the Adjustments tab, so a request sent while another tab was
 * showing used to vanish. It is parked in the store instead, the Adjustments tab is brought
 * forward, and the panel sends it as soon as it is mounted.
 */
export function askClaude(text: string) {
  useEditorStore.getState().setEditor({ pendingClaudeRequest: text });

  const ui = useUIStore.getState();
  if (ui.activePanel !== Panel.Adjustments) ui.setPanel(Panel.Adjustments);
  if (ui.uiVisibility && !ui.uiVisibility.rightPanel) {
    ui.setUI({ uiVisibility: { ...ui.uiVisibility, rightPanel: true } });
  }
}
