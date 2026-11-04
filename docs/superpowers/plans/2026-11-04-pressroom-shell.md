# Pressroom shell — implementation plan

**Status:** implemented · **Date:** 2026-11-04

Deckle's desktop app is the pressroom: it prepares books and papers, schedules delivery, and
manages readers. The newspaper typography belongs to its editorial previews; the surrounding
application uses quiet native-feeling controls.

## This slice

1. Replace the crowded top-level segmented control with a persistent left navigation rail.
2. Keep search, device status, Add, warnings, and Settings in a contextual top toolbar.
3. Give the active workspace a clear name and short purpose in the rail.
4. Preserve every existing tab, keyboard shortcut, menu, and in-progress operation.
5. Collapse the rail back into a compact top switcher below 860 points.
6. Use the shared warm paper, ink, rule, and brick palette; reduce decorative pills, borders,
   and card shadows in application chrome.

## Verification

- Frozen frontend install remains valid.
- TypeScript production and web builds pass.
- All 343 existing frontend tests pass.
- Workspaces retain ARIA tab state; the rail collapses below 860 points and Settings remains
  available in the compact toolbar.

