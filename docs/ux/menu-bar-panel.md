# UX spec: menu bar panel

Status: proposal v2, **macOS Wi-Fi menu style chosen by the user**, pending the owner's approval. No code changed. The first proposal (per-row layout, header summary, `⋯` menu, search) is replaced; see section 2.
Author: Uxie. Basis: the code in `app/Sources/Kubeport/` at version 0.1.0 (section 1), and the Idle timeout strings already on `main` (`app/Resources/*/Localizable.strings`). **Nobody has seen the interface on screen**: everything about rendering (heights, contrast, `ScrollView` behavior inside `MenuBarExtra`) is an estimate, marked "check on render". Reference mockup: canvas « Kubeport — maquettes du panneau », artboard "Style menu Wi-Fi" (https://claude.ai/artifact/38QmACGFo7oXPq9Lz7bvDV), which I could not open: the user's decisions written in section 2 are authoritative.

Documents read: `GLOSSARY.md`, ADR 0002 and 0003, README (Usage section), the five Swift files of the panel, `ffi/src/lib.rs`, and on `main` `core/src/manager.rs`, `app/Sources/Kubeport/L10n.swift` and `Localizable.strings` (Idle timeout). `PRODUCT.md`, `DESIGN.md`, `CLAUDE.md` and `AGENTS.md` do not exist in the repo: the spec relies on the glossary, the ADRs and the existing interface. No "Development cycle" section: the test commands cited in the DoD are those of the README (`scripts/measure-footprint.sh`).

## Background

The owner's request: "for the design, you can make me a proposal", with one precise question: **does the panel scroll with about twenty Forwards?**

Answer: yes, and badly today. With the current code, a row is about 36 pt (name + subtitle + spacing), a group title about 21 pt, and the list area caps at 420 pt. Twenty Forwards in three groups need about 800 pt: you see about ten rows, you must scroll for the rest, and the macOS scroll bar is hidden by default ("overlay" indicator). A Forward in Error on row fifteen is invisible: only the red dot on the icon signals it, without saying which one.

Decisions already made, not reopened: one port pair per Forward; 127.0.0.1 only; activating a Forward that shares a local port deactivates the other one (Competing Forwards) with a short notice; native look (now the macOS Wi-Fi menu style, chosen by the user); minimal memory; smooth, instant panel. ADR 0003 constraints: `MenuBarExtra(.window)`, no webview, ≤ 35 MB at idle with 5 Active Forwards (17 MB today).

Vocabulary: that of `GLOSSARY.md`. English terms, with the French term in parentheses: Forward, Target (Cible), Context (Contexte), Resolved pod (Pod résolu), Desired state Active/Inactive (État voulu Actif/Inactif), Actual state Connected/Reconnecting/Error (État réel Connecté/Reconnexion/Erreur), Competing Forwards (Forwards concurrents), Idle timeout (Délai d'inactivité, used by the core and by `main` but not yet in the glossary: open question 5). The words the glossary forbids appear in no interface text, in either language: tunnel, status, failure, port conflict, duplicate (as a noun for Competing Forwards), active pod, on/off, online (French: tunnel, statut, échec, conflit de port, doublon, pod actif, on/off, en ligne). "Namespace" stays as is (a Kubernetes term, not in the glossary). Copy addresses the user directly; the French copy uses "tu", like the current text (« Crée-en un »).

Languages: English is the app's base language and the French copy is the `fr` localization; the Copy section gives both. Reasons and some notices are emitted by the core (`core/`, `ffi/`) and are French strings today (for example `recherche d'un pod Ready`); the mockups show their English equivalents. Translating the core's strings is outside this spec.

---

## 1. Critique of the current panel

These findings describe the code at 0.1.0. Section 2 answers them with the Wi-Fi style.

Findings from the code, most severe first.

### Blockers (the user cannot finish or does not understand)

1. **False empty state at launch.** `ListScreen` shows « Aucun Forward. Crée-en un… » ("No Forwards. Create one…") as soon as `model.forwards` is empty. But the core only sends the first list after reading the login shell (up to 5 s, `ffi/src/lib.rs`, `run()`) and reactivating the Active Forwards. Meanwhile a user with 20 Forwards sees zero, and "New Forward" is enabled while the core loop is not answering yet: the form stays empty.
2. **Startup error: dead buttons.** `startError` is shown (invalid TOML config, a realistic case since the file is edited by hand), but "Open Config" and "Reload" do nothing: `configPath` comes from `core?.configPath()` and `core` is `nil`. This is exactly when the user needs to open the file. There is no way to retry without quitting the app.
3. **The switch has no name for VoiceOver.** `Toggle("", …).labelsHidden()`: VoiceOver announces "toggle button, on" without saying which Forward it belongs to. The colored dot is neither hidden nor described. A VoiceOver user cannot use the panel.
4. **Edit and Duplicate: right-click only.** The only hint is a tooltip, which appears after a delay and is not read by VoiceOver. No keyboard path. The README is the only documentation of the feature.
5. **The notice disappears when you change screens.** `notice` is rendered only by `ListScreen`. The `setLaunchAtLogin` notice (raised from Settings) and the "config not reloaded" notice are never seen if they arrive while the form or Settings is open. It disappears on its own after 6 s, with no button, and a second notice overwrites the first.
6. **Deletion: double-click = deletion.** The first click changes the label of the same button, in the same place, to « Confirmer la suppression » ("Confirm Deletion"). A double-click deletes. Nothing resets the confirmation.

### Fix before release (real friction)

7. **State carried by color alone for Connected.** Connected = green dot, Inactive = grey dot, with no text and no different shape. Reconnecting and Error have text in the subtitle; Connected does not. `.green` on a light background has low contrast (check on render).
8. **Density and readability at 20 Forwards.** Every row has two lines of text, including Inactive and Connected Forwards whose subtitle repeats `localhost:port`. No search, no collapsing of groups, group titles not pinned: at the bottom of the list you no longer know which Context you are in. An EKS Context name (`arn:aws:eks:…`) crushes the group title (`"\(context) · \(namespace)"`, a single line, no truncation rule).
9. **`.mini` switch.** Hit area of about 26 × 15 pt, on 20 tight rows: missed clicks, so the wrong Forward gets activated.
10. **A Forward in Error has no next step.** The cause requires "human action" (glossary), but the row offers neither "Try Again" nor "Edit": you have to switch it off and on again, or use a right-click nobody can discover. The reason is truncated to 2 lines, with no way to read it in full or copy it.
11. **Form: lists load with no indication.** `namespaces()` and `targets()` make network calls (`exec` auth is possible, so several seconds) with no timeout on the core side. While waiting, the `Picker` is empty or keeps the previous namespace's values (responses are not ordered: a late response can overwrite the right one). No spinner, no timeout.
12. **Form: unreachable Context.** The message `Namespaces non listés (…) : saisie libre.` ("Namespaces not listed (…): free entry.") appears at the bottom, in secondary grey, far from the field concerned, without a "Try Again" button. The text is a technical sentence, not an instruction.
13. **Form: empty cases with no message.** No Context in the kubeconfig: empty `Picker`, Save greyed out, no explanation. No Service in the namespace: the field becomes a free `TextField` without saying why. When editing, if the Forward's Context has disappeared from the kubeconfig (the "Context missing" case), the `Picker` has no valid selection.
14. **Form: late validation.** Name already used: error only after Save (the core rejects it with `DuplicateName`). Port 0 accepted by `UInt16`. Nothing warns that another Forward already shares the local port, or that editing an Active Forward restarts it (behavior documented only in the README).
15. **Crowded panel footer.** Six commands with the same visual weight on two lines, `.borderless` buttons that do not look like buttons. "Deactivate All" (no undo, and it rewrites `state.toml`) sits right next to "New Forward". "Reload" gives no feedback when everything is fine.
16. **Settings: "Save" only applies to the kubeconfig.** The launch-at-login switch applies immediately; the user thinks they must save for both. The hint « Réglages Système > Général > Ouverture » (System Settings > General > Login Items) is a path that macOS renames from one version to the next.
17. **Menu bar icon.** (a) The Error state is a non-template bitmap drawn with `NSColor.labelColor`: this color follows the app's appearance, not the menu bar's, which depends on the wallpaper. Risk of a dark icon on a dark bar (check on render, light and dark, varied wallpapers). (b) The red dot is the only signal, with no distinct shape. (c) `accessibilityDescription` = « Kubeport : erreur » ("Kubeport: error"), with no count.
18. **Screen state on reopening.** `@State screen` in `PanelView`: with `.window`, the view can survive the panel closing, and the panel then reopens on a half-filled form or at the previous scroll position (check on render).
19. **Unstable height.** The panel takes the height of its content: it jumps between the list, the form and Settings, and every time a row is added or removed, which moves buttons under the mouse.
20. **`Reconnecting` reasons on the first connection.** Activating a Forward first shows « Reconnexion : recherche d'un pod Ready » ("Reconnecting: looking for a Ready pod", `forward.rs:108`). To the user this is a first connection, not a reconnection. See open question 1.

### What works and stays

360 pt panel; grouping by Context and namespace; the switch as the main gesture; cascading form with lists filled from the cluster and an automatic `target:port` name; three-state icon; notification only on entering Error; Esc cancels the form; Return saves it; the "Forward deactivated: local port shared" notice.

---

## 2. Proposal: the macOS Wi-Fi menu style

The user chose the look of the macOS Tahoe Wi-Fi menu for the panel. Reference mockup: the canvas « Kubeport — maquettes du panneau », artboard "Style menu Wi-Fi" (https://claude.ai/artifact/38QmACGFo7oXPq9Lz7bvDV). I could not open it: this section, written from the user's decisions, is authoritative.

**What this replaces.** The first proposal (a state summary line in the header, search and state filter beyond 10 Forwards, collapsible groups with a summary, a `⋯` footer menu, a "Deactivate All" button with Undo, an action line under each Error row) is dropped: the Wi-Fi style chosen by the user answers the same needs differently (master switch, Active section first, "Inactive Forwards" disclosure, one scroll area).

Guiding principle: **the top of the panel answers "what is running, and is it healthy?", the master switch turns everything off and back on, and anything rare is one level deeper.**

### 2.1 Panel structure

Width **304 pt** (was 360). Top to bottom:

1. **Title row** (44 pt, fixed): bold title "Kubeport" on the left, the large **master switch** on the right (see 2.2).
2. **Thin inset separator.**
3. **List area**: ONE scroll area, height `min(content, 440 pt)`, containing in order the Active section, then the "Inactive Forwards ›" disclosure row and, when expanded, the Inactive section. Visible scroll indicator. The title row and the footer **never scroll**.
4. **Notice bar** (only when a notice is shown, above the footer, panel level, see 2.6).
5. **Separator, then footer**: two plain menu-style text rows (28 pt each, highlighted on hover like a menu item): "New Forward…" (⌘N) and "Kubeport Settings…" (⌘,).

Look: the translucent rounded panel is the system's (`MenuBarExtra(.window)` material and corners): draw no background of our own. Thin separators are inset by 12 pt; section headers and secondary text are grey (`.secondary`); nothing is hard-coded (see the theme DoD). Maximum panel height: about 44 + 440 + notice + 56 + separators, under 600 pt; safety cap: the screen's `visibleFrame` minus 200 pt. Open Config, Reload Config and Quit Kubeport leave the footer for the Settings screen (2.6).

### 2.2 Master switch

A regular-size switch (not the small row switch), at the right of the title.

| Situation | Switch | What a click does |
|---|---|---|
| At least one Forward Active | on | Deactivates **every** Forward and **remembers which were Active** |
| Nothing Active, a set remembered | off, enabled | Re-activates **the remembered set** |
| Nothing Active, nothing remembered | off, **greyed out** | Nothing (tooltip: "No Active Forward to deactivate or reactivate.") |
| Loading at launch, startup error, no Forward configured | off, greyed out | Nothing |

Rules:

- The switch value is derived: **on = at least one Forward is Active**. Activating a single Forward from its row while the master switch is off turns it on; the remembered set is kept until the next time the user turns the master switch off, which overwrites it with the then-Active set.
- No "Undo" notice: flipping the switch back is the undo. The earlier "N Forwards deactivated. Undo" notice and the "Deactivate All" button are gone.
- The remembered set is a list of Forward names kept in `UserDefaults` (survives quitting the app, like the Desired states in `state.toml`). A remembered name that no longer exists (deleted, renamed) is skipped silently; the set is emptied once re-activated.
- Re-activation goes through the normal activation path, one Forward at a time, so every Forward shows Reconnecting (looking for a Ready pod) then Connected, and an Error shows on its own row. Competing Forwards cannot be in the remembered set together (at most one of them was Active).
- VoiceOver: label "All Forwards", value "Active" or "Inactive" (never "on" or "off"), hint says what a click does (see Copy).

### 2.3 Active section and Inactive section

**Active section (top, always visible).** Only Active Forwards, in groups. Each group has a grey section header "{Context} · {namespace}" (pinned while scrolling, with a material background so rows do not show through). Long Context names (`arn:aws:eks:…`) are truncated in the middle on one line; the full name is the tooltip. Order: groups by Context then namespace, Forwards in config order inside a group (no sorting by state). When Forwards exist but none is Active, the section shows one muted line, "No Active Forwards." An Error or Reconnecting Forward is always in this section, so it can never be hidden below the fold or in a collapsed block.

**"Inactive Forwards ›" disclosure row** (like "Other Networks" in the Wi-Fi menu), right under the Active section, preceded by an inset separator. It shows the **count** of Inactive Forwards and a chevron (`›` collapsed, `⌄` expanded). A click, Return, Space or → / ← expands or collapses. The expanded state is remembered (`UserDefaults`, one boolean; default: collapsed) and never changes on its own. The row is hidden when there is no Inactive Forward.

**Inactive section (when expanded).** Inactive Forwards grouped by "{Context} · {namespace}" with the same grey headers; smaller **24 pt grey chips**, switch off.

**Moving between sections.** Flipping a row's switch moves the row to the other section, under its group (the group appears or disappears with it). The first proposal avoided any reordering so that rows would not jump under the mouse; the Wi-Fi style accepts it, and softens it: a 0.2 s move animation (none with "Reduce motion"), VoiceOver announces "{name} activated" or "{name} deactivated", and the keyboard focus follows the moved row. When the Inactive section is collapsed, a row deactivated from the Active section leaves the view and the count goes up by one. Check on render that this is not disorienting.

### 2.4 Behavior with 1, 5 and 20+ Forwards

| | 1 to 4 Forwards | 5 to 10 | 11 to 20+ |
|---|---|---|---|
| Title row, master switch, footer rows | yes | yes | yes |
| Active section | yes | yes | yes |
| "Inactive Forwards ›" | yes, collapsed by default | yes | yes |
| List height | content | content (≤ 440 pt) | content, **capped at 440 pt** |
| Scrolling | no | rarely | when the cap is reached (mostly with Inactive expanded) |

Estimates, to check on render (nobody has seen the app on screen): Active row 36 pt (48 with a one-line reason, 60 with two), Inactive row 32 pt, group header 24 pt, disclosure row 32 pt, open detail about 110 to 150 pt. The 20-Forward example of section 3 (6 Active in 3 groups, 14 Inactive): collapsed, about 330 pt, so **no scrolling**; expanded, about 800 pt, so the list is capped at 440 pt and scrolls (about 12 rows visible).

Scrolling: when the content passes 440 pt the whole list (Active section, disclosure row, Inactive section) scrolls **as one area**, with `.scrollIndicators(.visible)` so the bar is shown even without a trackpad. `LazyVStack(pinnedViews: [.sectionHeaders])` in a `ScrollView`. Check on render that the `ScrollView` takes the content height under the cap inside `MenuBarExtra`.

### 2.5 Anatomy of a Forward row

```
 (chip 28 pt)  Name ............... :port  [small switch]
               reason (Reconnecting and Error only)
```

- **Chip**: a circle, 28 pt in the Active section, 24 pt and grey in the Inactive section, with an SF Symbol glyph. Colour = state, and **the glyph shape differs too**, so colour is never the only signal:

  | State | Chip | Glyph (SF Symbol) | VoiceOver text | French |
  |---|---|---|---|---|
  | Connected | accent colour (`.tint`, blue by default) | `arrow.left.arrow.right` | "Connected" | « Connecté » |
  | Reconnecting | system orange | `arrow.triangle.2.circlepath` | "Reconnecting" | « Reconnexion » |
  | Error | system red | `exclamationmark` | "Error" | « Erreur » |
  | Inactive | grey, 24 pt | `arrow.left.arrow.right` | "Inactive" | « Inactif » |

  The glyph is static (no spinning), so no CPU at idle. Connected and Inactive share a glyph on purpose: they never sit in the same section, and the chip colour, size and switch differ. The chip is hidden from VoiceOver (the row label carries the state).
- **Name**: one line, truncated at the end, tooltip with the full name.
- **Reason**: under the name, only for Reconnecting and Error, in the state colour, 2 lines at most (the core's text, shown as is). The full text is in the open detail, selectable.
- **`:port`**: the local port, muted, monospaced digits, right-aligned before the switch. `localhost:` is only in the detail.
- **Small switch** on the right of **every** row (the user chose explicit switches over click-to-toggle). Size `.small`, hit area ≥ 32 × 18 pt (≥ 24 × 24 pt target). Label "Activate Forward {name}", value "Active" or "Inactive".
- **Click on the row, outside the switch**: opens the inline detail (2.5.1). **Right-click** keeps its menu: Edit…, Duplicate…, Copy Address, and Try Again on an Error row. Hovering highlights the row.

#### 2.5.1 Inline detail

The row expands in place, indented under the name. **One detail open at a time**: opening another closes the first; a second click on the row, Esc or leaving the panel closes it. Contents, in order:

- `Target`: `svc/api:80` or `pod/api-0:80`;
- `Resolved pod`: only when the Forward is Connected;
- `Address`: `localhost:8080` with a `Copy` button (`Copied` for 2 s, and the notice "Address copied.");
- `Auto-deactivate` (Idle timeout, « Désactivation auto »): the **countdown** "Inactive in 12 min", **only while the Forward is Active and its Idle timeout is running** (the core reports a deadline; none when there is an open connection or the Idle timeout is "never"). The value refreshes once a minute **only while the detail is open** (a `TimelineView(.periodic(from:, by: 60))` that exists only for the open detail; no timer when closed);
- on an Error row: `Try Again` (deactivates then reactivates the Forward on the Swift side), first among the buttons;
- if another Forward shares the local port: the "Competing Forward" sentence;
- buttons `Edit…` and `Duplicate…`.

### 2.6 Screens

**Form.** Header "‹ Forwards" (back, same as Cancel) on the left, the title on the right, in place of the title row (no master switch, no footer rows; the form's own Cancel and Save footer). Same cascade (Context → namespace → Type → Target → remote port → local port → Name) and the previous proposal's improvements, kept:

- **each dependent list shows its loading state inside the field** (spinner + "Loading namespaces…"), the field is disabled meanwhile, and stale responses are ignored (one request token per load);
- **10 s timeout** for namespaces and Targets: past that, an inline message under the field + "Try Again", and the field switches to free entry (explained);
- **inline, immediate validation**: name already used, port outside 1–65535; the greyed-out Save button has a tooltip that says what is missing;
- **information** (not an error) when the local port is already another Forward's: Competing Forwards;
- when editing an Active Forward: "This Forward is Active: it will restart when you save.";
- **separate deletion confirmation**: "Delete" opens a confirmation line ("Delete “name”?") with "Cancel" (default focus) and "Delete"; it closes after 5 s; if the Forward is Active: "It will be stopped first.";
- if the Forward's Context is no longer in the kubeconfig, it stays selected with the suffix "(missing from kubeconfig)" and an inline message.

**Settings.** Header "‹ Forwards" + "Settings". The launch-at-login switch applies **immediately**; Kubeconfig has its field, "Choose…" (`NSOpenPanel`) and its own "Apply" button. **New here: three plain menu-style rows, moved from the old footer:** "Open Config", "Reload Config", then after an inset separator "Quit Kubeport"; the version line at the bottom. Settings stays reachable when the core did not start (so Quit and Open Config remain available during a startup error); "Reload Config" and "Apply" are then greyed out. Esc and the back button leave the screen. (The Auto-deactivate fields of the form and of Settings already exist on main, with their strings in `Localizable.strings`; this spec does not redesign them.)

**Notice.** A bar above the footer, **displayed by `PanelView`** (so on every screen), `info.circle` icon, 3 lines at most, ✕ button. Duration `max(6 s, 0.06 s × number of characters)`, capped at 20 s. Each new notice is announced to VoiceOver (`AccessibilityNotification.Announcement`).

**Menu bar icon.** Unchanged from the first proposal: three states (outline / filled / filled with a red dot); dynamic `accessibilityDescription` ("Kubeport: no Active Forwards", "Kubeport: 5 Active Forwards", "Kubeport: 5 Active Forwards, 2 in Error"); the Error state to validate on light and dark bars and varied wallpapers, with a template-symbol fallback (a cut-out exclamation badge) if unreadable; no distinct state for Reconnecting.

### 2.7 Keyboard

macOS does not move keyboard focus to buttons and switches with Tab unless "Full Keyboard Access" (« Accès au clavier complet ») is on (off by default). The panel provides its own navigation over one flat order: **master switch → Active rows (with their open detail buttons) → "Inactive Forwards" row → Inactive rows → "New Forward…" → "Kubeport Settings…"**. Section headers are not focusable.

| Key | Action |
|---|---|
| ↑ / ↓ | Move focus along the order above |
| Space | On a Forward row or the master switch: flip the switch. On a footer row or a detail button: press it |
| Return | On a Forward row: open or close its detail. On the disclosure row, a footer row or a detail button: activate it |
| → / ← on the "Inactive Forwards" row | Expand / collapse |
| ⌘N | New Forward |
| ⌘, | Kubeport Settings |
| ⌘R | Reload config |
| ⌘Q | Quit |
| Esc | Closes the open detail, then closes the panel |

On opening, **nothing is focused**: the focus ring appears on the first ↑ / ↓, so a stray Space cannot flip a Forward. In the form and Settings, Tab follows the visual order from top to bottom. The search shortcut ⌘F of the first proposal is dropped with the search.

### 2.8 Memory and instant opening

- No derived data (groups, counters, Active/Inactive split) computed in `body`: `CoreModel` recomputes it on each `forwardsChanged` (once per event, not once per render).
- `LazyVStack`: only the visible rows are instantiated; a collapsed Inactive section is a count, not 14 views.
- No timer and no animation at idle; the only timer is the 60 s refresh of an **open** detail's countdown. No bitmap image: SF Symbols and system shapes only. No hand-drawn blur: the system panel material.
- Persistent state added, all in `UserDefaults`: the Inactive section's expanded boolean and the master switch's remembered Forward names.
- No network call or wait in the opening path; the form's lists load after the form is displayed.
- The open detail and the screen are view state (`@State`), reset on each opening.

---

## 3. Mockups

Legend (the mockups are to scale in width, not in height; every box is 52 characters wide):

- Chips: `(⇄)` Connected (accent, 28 pt), `(↻)` Reconnecting (orange), `(!)` Error (red), `[⇄]` Inactive (grey, 24 pt).
- Switches: `[ ●]` Active, `[● ]` Inactive, `[░ ]` greyed out. The title row's switch is the large master switch.
- Group headers (`eks-prod · payments`) and `:port` are grey. `┃` is the scroll-bar thumb, `╎` its track. `›` collapsed, `⌄` expanded.

### 3.1 List, 20 Forwards (6 Active, 14 Inactive), Inactive collapsed

Everything fits: no scrolling, the Error is visible without any action.

```
┌──────────────────────────────────────────────────┐
│ Kubeport                                  [ ●]   │
├──────────────────────────────────────────────────┤
│ eks-prod · payments                              │
│ (⇄) ledger-db                      :5432  [ ●]   │
│ (⇄) ledger-ro                      :5433  [ ●]   │
│ (↻) grafana                        :3000  [ ●]   │
│     looking for a Ready pod                      │
│ eks-staging · api                                │
│ (⇄) api                            :8080  [ ●]   │
│ (!) pg-staging                     :5432  [ ●]   │
│     authentication refused: token expired        │
│ docker-desktop · postiz                          │
│ (⇄) postiz-web                     :5000  [ ●]   │
│   ────────────────────────────────────────────   │
│ Inactive Forwards                        14  ›   │
├──────────────────────────────────────────────────┤
│ New Forward…                                ⌘N   │
│ Kubeport Settings…                          ⌘,   │
└──────────────────────────────────────────────────┘
```

### 3.2 Same list, "Inactive Forwards" expanded: the 440 pt cap is reached

The title row and the footer stay put; the whole list scrolls as one area; the thumb shows about half of it is visible.

```
┌──────────────────────────────────────────────────┐
│ Kubeport                                  [ ●]   │
├──────────────────────────────────────────────────┤
│ eks-prod · payments                            ┃ │
│ (⇄) ledger-db                      :5432  [ ●] ┃ │
│ (⇄) ledger-ro                      :5433  [ ●] ┃ │
│ (↻) grafana                        :3000  [ ●] ┃ │
│     looking for a Ready pod                    ┃ │
│ eks-staging · api                              ┃ │
│ (⇄) api                            :8080  [ ●] ┃ │
│ (!) pg-staging                     :5432  [ ●] ┃ │
│     authentication refused: token expired      ┃ │
│ docker-desktop · postiz                        ┃ │
│ (⇄) postiz-web                     :5000  [ ●] ┃ │
│   ──────────────────────────────────────────── ┃ │
│ Inactive Forwards                        14  ⌄ ┃ │
│ eks-prod · payments                            ┃ │
│ [⇄] redis-cache                    :6379  [● ] ╎ │
│ [⇄] kafka-ui                       :8080  [● ] ╎ │
│ [⇄] prometheus                     :9090  [● ] ╎ │
│ [⇄] vault                          :8200  [● ] ╎ │
│ [⇄] jaeger                        :16686  [● ] ╎ │
│ eks-staging · api                              ╎ │
│ [⇄] api-debug                      :5005  [● ] ╎ │
│ [⇄] pg-replica                     :5433  [● ] ╎ │
├──────────────────────────────────────────────────┤
│ New Forward…                                ⌘N   │
│ Kubeport Settings…                          ⌘,   │
└──────────────────────────────────────────────────┘
```

### 3.3 Master switch states

At least one Forward Active (switch on):

```
┌──────────────────────────────────────────────────┐
│ Kubeport                                  [ ●]   │
└──────────────────────────────────────────────────┘
```

Nothing Active, a set remembered (switch off, enabled):

```
┌──────────────────────────────────────────────────┐
│ Kubeport                                  [● ]   │
└──────────────────────────────────────────────────┘
```

Nothing Active, nothing remembered, or loading, or no core (switch off, greyed out):

```
┌──────────────────────────────────────────────────┐
│ Kubeport                                  [░ ]   │
└──────────────────────────────────────────────────┘
```

### 3.4 Forwards exist, none Active

```
┌──────────────────────────────────────────────────┐
│ Kubeport                                  [░ ]   │
├──────────────────────────────────────────────────┤
│                                                  │
│ No Active Forwards.                              │
│                                                  │
│   ────────────────────────────────────────────   │
│ Inactive Forwards                        20  ›   │
├──────────────────────────────────────────────────┤
│ New Forward…                                ⌘N   │
│ Kubeport Settings…                          ⌘,   │
└──────────────────────────────────────────────────┘
```

### 3.5 Inline details: Connected with countdown, Error, Inactive with a Competing Forward

Excerpt of the list; one detail is open at a time in practice, three are shown here for comparison.

```
┌──────────────────────────────────────────────────┐
│ eks-staging · api                                │
│ (⇄) api                            :8080  [ ●]   │
│     Target           svc/api:80                  │
│     Resolved pod     api-7d9f6c-x2k4j            │
│     Address          localhost:8080     [Copy]   │
│     Auto-deactivate  Inactive in 12 min          │
│     [Edit…]  [Duplicate…]                        │
│   ────────────────────────────────────────────   │
│ (!) pg-staging                     :5432  [ ●]   │
│     authentication refused: token expired        │
│     Target           svc/pg:5432                 │
│     Address          localhost:5432     [Copy]   │
│     [Try Again]  [Edit…]  [Duplicate…]           │
│   ────────────────────────────────────────────   │
│ [⇄] pg-replica                     :5433  [● ]   │
│     Target           svc/pg-ro:5432              │
│     Address          localhost:5433     [Copy]   │
│     Competing Forward: “ledger-ro” uses          │
│     the same local port. Only one of the two     │
│     is Active.                                   │
│     [Edit…]  [Duplicate…]                        │
└──────────────────────────────────────────────────┘
```

### 3.6 Empty state (no Forward configured)

```
┌──────────────────────────────────────────────────┐
│ Kubeport                                  [░ ]   │
├──────────────────────────────────────────────────┤
│                                                  │
│                                                  │
│                       ⇄                          │
│                                                  │
│                  No Forwards                     │
│                                                  │
│   A Forward connects a local port to a Service   │
│   or a Pod in a Kubernetes cluster.              │
│                                                  │
│             [  Create a Forward  ]               │
│                                                  │
│            Or edit the config file               │
│                 Open Config                      │
│                                                  │
├──────────────────────────────────────────────────┤
│ New Forward…                                ⌘N   │
│ Kubeport Settings…                          ⌘,   │
└──────────────────────────────────────────────────┘
```

### 3.7 Loading at launch (the core's first send not received yet)

```
┌──────────────────────────────────────────────────┐
│ Kubeport                                  [░ ]   │
├──────────────────────────────────────────────────┤
│                                                  │
│             Loading Forwards…                    │
│                                                  │
├──────────────────────────────────────────────────┤
│ New Forward… (greyed out)                   ⌘N   │
│ Kubeport Settings…                          ⌘,   │
└──────────────────────────────────────────────────┘
```

### 3.8 Startup error (unreadable config)

```
┌──────────────────────────────────────────────────┐
│ Kubeport                                  [░ ]   │
├──────────────────────────────────────────────────┤
│ (!) Kubeport couldn't read its config            │
│                                                  │
│     expected `=` at line 12 column 5             │
│     (config.toml)                                │
│                                                  │
│     [ Open Config ]   [ Try Again ]              │
│                                                  │
├──────────────────────────────────────────────────┤
│ New Forward… (greyed out)                   ⌘N   │
│ Kubeport Settings…                          ⌘,   │
└──────────────────────────────────────────────────┘
```

### 3.9 Form (creation, Services loading)

```
┌──────────────────────────────────────────────────┐
│ ‹ Forwards                         New Forward   │
├──────────────────────────────────────────────────┤
│ Context                                          │
│ [ eks-staging                              ▾ ]   │
│ Namespace                                        │
│ [ api                                      ▾ ]   │
│ Type                                             │
│ [ Service | Pod ]                                │
│ Service                                          │
│ [ ◌ Loading Services…                        ]   │
│ Remote port            Local port                │
│ [ —          ▾ ]   [                    ]        │
│   Default: the remote port                       │
│ Name                                             │
│ [ api:80                                     ]   │
│   Suggested: target:port                         │
├──────────────────────────────────────────────────┤
│                               [Cancel]  [Save]   │
└──────────────────────────────────────────────────┘
```

Unreachable Context (excerpt):

```
┌──────────────────────────────────────────────────┐
│ ‹ Forwards                         New Forward   │
├──────────────────────────────────────────────────┤
│ Context         [ eks-prod                 ▾ ]   │
│ Namespace       [ payments                   ]   │
│ (!) Context “eks-prod” unreachable: timed out    │
│     (10 s). Enter the namespace manually or      │
│     try again.   [Try Again]                     │
│ …                                                │
└──────────────────────────────────────────────────┘
```

### 3.10 Settings

```
┌──────────────────────────────────────────────────┐
│ ‹ Forwards                            Settings   │
├──────────────────────────────────────────────────┤
│ Launch Kubeport at login                  [ ●]   │
│                                                  │
│ Kubeconfig                                       │
│ [ /Users/deki/.kube/config       ] [Choose…]     │
│   Leave empty to use the default kubeconfig.     │
│   [Apply]                                        │
├──────────────────────────────────────────────────┤
│ Open Config                                      │
│ Reload Config                                    │
│   ────────────────────────────────────────────   │
│ Quit Kubeport                                    │
├──────────────────────────────────────────────────┤
│ Kubeport 0.1.0                                   │
└──────────────────────────────────────────────────┘
```

---

## 4. UX spec (team template)

### Flows

1. **Activate a Forward.** Open the panel → "Inactive Forwards ›" → expand → find the row → flip its small switch (or focus it and press Space) → the row moves to the Active section under its group with an orange chip and "looking for a Ready pod", then a blue chip. If a Competing Forward was Active, it moves to the Inactive section and a notice says so.
2. **Check that everything is fine.** Open the panel → read the Active section: chip colours and glyph shapes, a reason line under any Reconnecting or Error row. No scrolling needed unless there are more than about 10 Active Forwards.
3. **Handle an Error.** Red chip with its reason → click the row → "Try Again" (transient cause) or "Edit…" (port in use, Target not found) → Save (the Active Forward restarts).
4. **Turn everything off, then back on.** Master switch off → every Forward moves to the Inactive section, the switch stays enabled → master switch on → the remembered Forwards come back Active, one after the other.
5. **Create a Forward.** "New Forward…" (⌘N) → Context → namespace → Type → Target → remote port → local port → Name → Save. Back to the list, row added to the Inactive section under its group (the Desired state is not Active by default: question 3).
6. **Edit or duplicate.** Click the row → detail → "Edit…" or "Duplicate…" (also right-click, or VoiceOver > Actions). Duplicate suggests "{name}-copy".
7. **Delete.** Edit → Delete → confirmation line → Delete.
8. **Copy the address.** Click the row → "Copy" next to `localhost:{port}`.
9. **Idle timeout.** An Active Forward with no open connection → its detail shows "Inactive in 12 min" → at expiry the row moves to the Inactive section and a notice says "{name} deactivated after {duration} with no connection".
10. **Settings and config.** "Kubeport Settings…" (⌘,) → launch switch applies at once; Kubeconfig → Apply; "Open Config", "Reload Config", "Quit Kubeport" are rows on this screen.
11. **Unreadable config.** Screen 3.8 → Open Config → fix it → Try Again. Quit remains reachable through "Kubeport Settings…".
12. **First use.** Empty state 3.6 → Create a Forward → flow 5.

### Screen states

| Screen | Empty | Loading | Error | Success |
|---|---|---|---|---|
| Title row, master switch | greyed out when nothing is Active and nothing remembered (3.3) | greyed out until the core's first send | greyed out when the core did not start | on = at least one Forward Active |
| Active section | "No Active Forwards." when Forwards exist but none is Active (3.4); the whole empty state 3.6 when there are none | 3.7: "Loading Forwards…", never the empty state before the first send | 3.8 if the core did not start; red chip and reason for a Forward in Error | grouped rows with blue chips |
| Inactive section | row hidden when there is no Inactive Forward | not shown | not applicable | count on the disclosure row; grouped grey rows when expanded |
| Row detail | not applicable | not applicable | "Resolved pod" absent if not Connected; `Try Again` on an Error row; no countdown line when no deadline | Target, Resolved pod, Address, countdown, actions |
| Footer rows | "New Forward…" enabled (it is the way out of the empty state) | "New Forward…" greyed out; Settings enabled | same | enabled |
| Form: Context | "No Context in the kubeconfig." + "Open Settings" | local list, instant | Context missing (editing): kept, with "(missing from kubeconfig)" | Context selected |
| Form: namespace and Target | "No Service in “{ns}”." (+ ExternalName note), switching to Pod offered | spinner in the field, field disabled, 10 s timeout | inline message + "Try Again" + free entry | list filled, first item selected |
| Form: saving | Save greyed out while incomplete, tooltip says what is missing | Save replaced by a spinner during `saving` | core error under the button, entered text kept | back to the list |
| Settings | empty kubeconfig = default, explicit placeholder | reading the kubeconfig: field disabled during the read | red error under the field; launch-at-login notice; "Reload Config" and "Apply" greyed out without a core | switch applied at once; "Kubeconfig applied." |
| Notice | not applicable | not applicable | stays until closed or its duration ends | auto-closes |

### Copy

Glossary words respected. `{…}` are values. "Namespace" kept as is. English is the base language; the French column is the `fr` localization (French typography: « » quotes, space before a colon, "tu"). *Core text* means a string emitted by the core and shown as is in both languages. Plural forms go through a stringsdict. Strings already on main (`Localizable.strings`) are reused as written: `idle.countdown`, `notice.idleDeactivated`, `form.idleTimeout`.

**Title row, list and footer**

| Where | English | French |
|---|---|---|
| Title | `Kubeport` | `Kubeport` |
| Master switch, label | `All Forwards` | `Tous les Forwards` |
| Master switch, value | `Active` / `Inactive` | `Actif` / `Inactif` |
| Master switch, hint (Active) | `Deactivates all Forwards and remembers which were Active.` | `Désactive tous les Forwards et retient lesquels étaient Actifs.` |
| Master switch, hint (Inactive) | `Reactivates the {n} Forwards that were Active.` (`the Forward` for 1) | `Réactive les {n} Forwards qui étaient Actifs.` (`le Forward` pour 1) |
| Master switch, tooltip when greyed out | `No Active Forward to deactivate or reactivate.` | `Aucun Forward Actif à désactiver ou à réactiver.` |
| Group header | `{context} · {namespace}` (tooltip: the full text) | `{contexte} · {namespace}` |
| Active section, none Active | `No Active Forwards.` | `Aucun Forward Actif.` |
| Disclosure row | `Inactive Forwards` + count | `Forwards Inactifs` + nombre |
| Disclosure row, VoiceOver | `Inactive Forwards, {n}, collapsed` / `expanded` | `Forwards Inactifs, {n}, réduit` / `développé` |
| Footer rows | `New Forward…` · `Kubeport Settings…` | `Nouveau Forward…` · `Réglages de Kubeport…` |
| `New Forward…` tooltip when greyed out | `Available once the config is loaded.` | `Disponible une fois la config chargée.` |

**Forward row**

| Where | English | French |
|---|---|---|
| Switch, label | `Activate Forward {name}` | `Activer le Forward {nom}` |
| Switch, value | `Active` / `Inactive` | `Actif` / `Inactif` |
| Row VoiceOver label | `{name}, {Connected / Reconnecting / Error / Inactive}, local port {port}` | `{nom}, {Connecté / Reconnexion / Erreur / Inactif}, port local {port}` |
| VoiceOver announcement on a move | `{name} activated` / `{name} deactivated` | `{nom} activé` / `{nom} désactivé` |
| Reconnecting / Error reason | *core text*, as is | *core text* |
| Detail: labels | `Target` · `Resolved pod` · `Address` · `Auto-deactivate` | `Cible` · `Pod résolu` · `Adresse` · `Désactivation auto` |
| Detail: values | `svc/{name}:{port}` or `pod/{name}:{port}` · `{pod}` · `localhost:{port}` · `Inactive in {duration}` (`{n} min`, `{n} h`; at least 1 min) | same · `Inactif dans {durée}` |
| Detail: buttons | `Copy` (then `Copied` for 2 s) · `Edit…` · `Duplicate…` · `Try Again` (Error only) | `Copier` (puis `Copié` 2 s) · `Modifier…` · `Dupliquer…` · `Réessayer` (Erreur seulement) |
| Competing Forwards | `Competing Forward: “{other}” uses the same local port. Only one of the two is Active.` | `Forward concurrent : « {autre} » utilise le même port local. Un seul des deux est Actif.` |
| Right-click menu / VoiceOver actions | `Activate` or `Deactivate` · `Edit…` · `Duplicate…` · `Copy Address` · `Try Again` (if Error) | `Activer` ou `Désactiver` · `Modifier…` · `Dupliquer…` · `Copier l'adresse` · `Réessayer` (si Erreur) |
| Duplicate: suggested name | `{name}-copy` | `{nom}-copie` |

**Empty and error states**

| Where | English | French |
|---|---|---|
| Empty: title | `No Forwards` | `Aucun Forward` |
| Empty: explanation | `A Forward connects a local port to a Service or a Pod in a Kubernetes cluster.` | `Un Forward relie un port local à un Service ou à un Pod d'un cluster Kubernetes.` |
| Empty: buttons | `Create a Forward` · `Open Config` | `Créer un Forward` · `Ouvrir la config` |
| Empty: line above the link | `Or edit the config file` | `Ou modifie le fichier de config` |
| Loading | `Loading Forwards…` | `Chargement des Forwards…` |
| Startup error: title | `Kubeport couldn't read its config` | `Kubeport n'a pas pu lire sa config` |
| Startup error: body | *core text*: the reason, monospaced, selectable | *core text* |
| Startup error: buttons | `Open Config` · `Try Again` | `Ouvrir la config` · `Réessayer` |

**Notices (final text)**

| Situation | English | French |
|---|---|---|
| Competing Forward deactivated (core: `{rival} désactivé : port local partagé avec {name}`) | *core text*, as is; the interface puts both names in bold | *core text* |
| Idle timeout expired | `{name} deactivated after {duration} with no connection` | `{nom} désactivé après {durée} sans connexion` |
| Config not reloaded (core) | *core text*, as is | *core text* |
| Config reloaded (Could, needs the core) | `Config reloaded: {n} Forwards.` | `Config relue : {n} Forwards.` |
| Launch at login refused | `Launch at login: {reason}` | `Lancement à l'ouverture de session : {raison}` |
| Copy | `Address copied.` | `Adresse copiée.` |
| Closing the notice (accessibility) | `Close Notice` | `Fermer l'avis` |

The master switch raises no notice.

**Form**

| Where | English | French |
|---|---|---|
| Back | `‹ Forwards` (accessible label: `Back to the Forwards list`) | `‹ Forwards` (étiquette accessible : `Retour à la liste des Forwards`) |
| Titles | `New Forward` · `Edit Forward` · `Duplicate Forward` | `Nouveau Forward` · `Modifier le Forward` · `Dupliquer le Forward` |
| Fields | `Context` · `Namespace` · `Type` (`Service` / `Pod`) · `Service` or `Pod` · `Remote port` · `Local port` · `Name` | `Contexte` · `Namespace` · `Type` (`Service` / `Pod`) · `Service` ou `Pod` · `Port distant` · `Port local` · `Nom` |
| Loading | `Loading namespaces…` · `Loading Services…` · `Loading Pods…` | `Chargement des namespaces…` · `Chargement des Services…` · `Chargement des Pods…` |
| Local port help | `Default: the remote port.` | `Par défaut : le port distant.` |
| Name help | `Suggested: target:port.` | `Proposé : cible:port.` |
| No Context | `No Context in the kubeconfig. Add one with kubectl, or choose another kubeconfig in Settings.` + `Open Settings` | `Aucun Contexte dans le kubeconfig. Ajoute-en un avec kubectl, ou choisis un autre kubeconfig dans les Réglages.` + `Ouvrir les Réglages` |
| Context missing | `{context} (missing from kubeconfig)` + `This Context no longer exists in the kubeconfig: choose another one.` | `{contexte} (absent du kubeconfig)` + `Ce Contexte n'existe plus dans le kubeconfig : choisis-en un autre.` |
| Cluster unreachable | `Context “{context}” unreachable: {reason}. Enter the namespace manually or try again.` (same with `the Target`) + `Try Again` | `Contexte « {contexte} » injoignable : {raison}. Saisis le namespace à la main ou réessaie.` (idem avec `la Cible`) + `Réessayer` |
| Timed out | `Context “{context}” unreachable: timed out (10 s). …` | `Contexte « {contexte} » injoignable : délai dépassé (10 s). …` |
| No namespace listed (permissions) | `You don't have permission to list namespaces: enter it manually.` | `Tu n'as pas le droit de lister les namespaces : saisis-le à la main.` |
| No Target | `No Service in “{namespace}”.` + `ExternalName Services and Services without a selector are not offered.` + `Choose a Pod` button | `Aucun Service dans « {namespace} ».` + `Les Services ExternalName et sans selector ne sont pas proposés.` + bouton `Choisir un Pod` |
| Name already taken | `A Forward already has this name.` | `Un Forward porte déjà ce nom.` |
| Invalid port | `Enter a port between 1 and 65535.` | `Entre un port entre 1 et 65535.` |
| Competing Forwards (information) | `Competing Forward: “{other}” also uses port {port}. Only one of the two will be Active.` | `Forward concurrent : « {autre} » utilise aussi le port {port}. Un seul des deux sera Actif.` |
| Editing an Active Forward | `This Forward is Active: it will restart when you save.` | `Ce Forward est Actif : il sera relancé à l'enregistrement.` |
| Save greyed out (tooltip) | `Missing: {list of missing fields}.` | `Complète : {liste des champs manquants}.` |
| Buttons | `Cancel` · `Save` | `Annuler` · `Enregistrer` |
| Deletion | `Delete` → line: `Delete “{name}”?` + `Cancel` · `Delete`; if Active: `It will be stopped first.` | `Supprimer` → ligne : `Supprimer « {nom} » ?` + `Annuler` · `Supprimer` ; si Actif : `Il sera d'abord arrêté.` |
| Core error on save | *core text*, under the buttons | *core text* |

**Settings**

| Where | English | French |
|---|---|---|
| Title | `Settings` | `Réglages` |
| Switch | `Launch Kubeport at login` | `Lancer Kubeport à l'ouverture de session` |
| Needs approval | `macOS needs your approval.` + `Open System Settings` button (`SMAppService.openSystemSettingsLoginItems()`) | `macOS demande ton accord.` + bouton `Ouvrir Réglages Système` |
| Field | `Kubeconfig`; placeholder `Default: KUBECONFIG from the shell, then ~/.kube/config` | `Kubeconfig` ; placeholder `Par défaut : KUBECONFIG du shell, puis ~/.kube/config` |
| Buttons | `Choose…` · `Apply` | `Choisir…` · `Appliquer` |
| Help | `Leave empty to use the default kubeconfig.` | `Laisse vide pour utiliser le kubeconfig par défaut.` |
| Menu-style rows | `Open Config` · `Reload Config` · `Quit Kubeport` | `Ouvrir la config` · `Recharger la config` · `Quitter Kubeport` |
| Version line | `Kubeport {version}` | `Kubeport {version}` |
| Success | `Kubeconfig applied.` | `Kubeconfig appliqué.` |

**Menu bar icon (accessible label and tooltip)**

| Language | Text |
|---|---|
| English | `Kubeport: no Active Forwards` · `Kubeport: {n} Active Forward` / `{n} Active Forwards` · `Kubeport: {n} Active Forwards, {k} in Error` |
| French | `Kubeport : aucun Forward Actif` · `Kubeport : {n} Forward Actif` / `{n} Forwards Actifs` · `Kubeport : {n} Forwards Actifs, {k} en Erreur` |

### Accessibility

- Each Forward row is a VoiceOver container (`.accessibilityElement(children: .contain)`) labelled `{name}, {state}, local port {port}`, followed by the reason when there is one; the chip is hidden from VoiceOver.
- The row switch has the label `Activate Forward {name}` and the value Active/Inactive; the master switch has the label `All Forwards`, the value Active/Inactive and a hint that says what a click does. Neither ever says "on" or "off".
- Group headers carry the header trait. The "Inactive Forwards" row is a button announcing its count and its expanded or collapsed state (`accessibilityExpanded`).
- Custom VoiceOver actions on each row: Activate/Deactivate, Edit, Duplicate, Copy Address, Try Again (if Error). A move between sections is announced.
- State is never carried by colour alone: the four states have distinct chip glyphs, plus the reason text under Reconnecting and Error. This matters most when the Mac's accent colour is red or orange: the Connected chip then shares a hue with Error or Reconnecting, and only the glyph and the reason tell them apart.
- Full keyboard navigation with the table in 2.7, **without** "Full Keyboard Access"; the master switch and the footer rows are in the order.
- Visible focus on the row, the master switch, the disclosure row and the footer rows (ring or highlight, contrast ≥ 3:1).
- Contrast: text ≥ 4.5:1; chip glyph against its chip and the chip against the panel ≥ 3:1 in light and dark, with "Increase contrast" on (a white glyph on system orange is at risk: see open question 6).
- Semantic fonts (`.body`, `.caption`), no fixed size. The only animation is the 0.2 s row move, off with "Reduce motion".
- Targets ≥ 24 × 24 pt (`.small` switch, text buttons with a 4 pt margin); footer rows 28 pt high.
- Each notice is announced to VoiceOver and `Close Notice` is reachable.
- In the form: each field has a label, inline messages are tied to their field, and a validation error is announced when it appears.

### Sketch

See section 3 (mockups 3.1 to 3.10).

### DoD additions

Swift unit tests (target `KubeportTests`, to be created) unless stated otherwise.

- [ ] **macOS theme**: the panel, the form, Settings and the icon follow light or dark mode and the Mac's accent colour, without relaunching the app. No hard-coded colour: system colours only (`.primary`, `.secondary`, `Color(nsColor: .systemRed)`, system materials, default `.tint`); the panel background and corners are the system's. Checked on screen in light, in dark, and with an accent colour other than blue. Reference mockup: the canvas « Kubeport — maquettes du panneau », artboard "Style menu Wi-Fi", Theme control (Auto, Light, Dark).
- [ ] The panel is 304 pt wide; the title row shows the bold title "Kubeport" and the master switch; the footer shows exactly "New Forward…" and "Kubeport Settings…" (XCUITest on the element tree).
- [ ] The model does not produce the empty state before the first `forwardsChanged`: test on `CoreModel.hasLoaded`; while false, the list shows "Loading Forwards…", the master switch and "New Forward…" are greyed out.
- [ ] With `startError`, "Open Config" opens `config.toml` (path computed without `core`) and "Try Again" recreates the core; test on `CoreModel.retryStart()`. In that state "Kubeport Settings…" is enabled and Settings shows "Quit Kubeport" enabled and "Reload Config" greyed out.
- [ ] Master switch, model tests with a fake core: with 3 Forwards Active, turning it off deactivates all 3 and stores their names; turning it on re-activates exactly those 3; a stored name that was deleted is skipped; with nothing Active and nothing stored it is greyed out; the stored set survives re-creating `CoreModel` (`UserDefaults`); activating one Forward while it is off turns it on and keeps the stored set until the next "off".
- [ ] The master switch has the label `All Forwards`, the value Active/Inactive and the hint of Copy; no VoiceOver text says "on" or "off".
- [ ] The Active/Inactive split and the groups are computed in `CoreModel` on each `forwardsChanged`; test with 20 Forwards (6 Active in 3 groups, 14 Inactive in 3 groups): the right rows, groups ordered by Context then namespace, a Forward in Error or Reconnecting is always in the Active section.
- [ ] The list is ONE scroll area: with 20 Forwards and Inactive expanded it is exactly 440 pt high, its scroll indicator is visible (`scrollBars.firstMatch`), and after scrolling to the bottom the title row and the footer rows keep the same frames (XCUITest); with the Inactive section collapsed and 6 Active it takes its content height (< 440 pt).
- [ ] "Inactive Forwards" shows the count, is hidden at 0, defaults to collapsed, and its expanded state survives closing and reopening the panel and relaunching the app (test on the `UserDefaults` key).
- [ ] Chips: 28 pt in the Active section, 24 pt and grey in the Inactive section; the four states have different glyphs (test on `glyph(for:)`); screenshots in light and dark, and with a red accent colour, reviewed: Connected and Error stay distinguishable by glyph and reason.
- [ ] Each row has the label `{name}, {state}, local port {port}` and a switch `Activate Forward {name}` on **every** row (Active and Inactive); Accessibility Inspector > Audit reports no issue on the list screen.
- [ ] Clicking a row outside its switch opens the detail (Target, Address, "Edit…", "Duplicate…", and Resolved pod only if Connected); opening another closes the first; clicking the switch does not open the detail.
- [ ] The countdown line appears only for an Active Forward whose status carries an idle deadline, reads "Inactive in {duration}" (at least 1 min), and its refresh timer exists only while the detail is open (test: no timer when closed).
- [ ] A Forward in Error shows `Try Again` in its detail; it chains `deactivate` then `activate` on the same name (fake core).
- [ ] Flipping a row's switch moves the row to the other section, the count updates, VoiceOver receives the announcement, and no animation runs with "Reduce motion" on.
- [ ] Keyboard, XCUITest with "Full Keyboard Access" off: nothing focused on opening; ↑ / ↓ follow master switch → Active rows → disclosure → Inactive rows → footer rows; Space flips a switch, Return opens a detail; ⌘N, ⌘, ⌘R, ⌘Q work; Esc closes the detail, then the panel.
- [ ] The notice shows on all three screens (list, form, Settings), closes with ✕, lasts `max(6 s, 0.06 s × characters)` capped at 20 s, and is announced by VoiceOver (test on posting `AccessibilityNotification.Announcement`).
- [ ] Form: loading shows the spinner, disables the field, ignores a stale response (two racing loads, the first answer arrives last) and switches to an inline message + "Try Again" after 10 s (injected clock).
- [ ] Form: "A Forward already has this name." appears before Save; a port outside 1–65535 is rejected; the "Competing Forward" information appears when the local port is shared.
- [ ] Form: deletion goes through the confirmation line, initial focus on "Cancel", the line closes after 5 s; a double-click on "Delete" does not delete.
- [ ] Settings: the launch switch applies without Save; Kubeconfig has its own "Apply"; "Open Config", "Reload Config" and "Quit Kubeport" are rows of this screen and are gone from the footer.
- [ ] The icon has a dynamic accessible label for the three states (test on `StatusIcon.description(activeCount:errorCount:)`); the Error state is readable on light and dark bars (manual screenshots attached to the PR).
- [ ] On each opening of the panel: back to the list, no detail open (by hand: close by clicking outside with a form open, then reopen).
- [ ] Memory: `scripts/measure-footprint.sh` with 5 Active Forwards stays ≤ 35 MB, then with a 50-Forward config and the panel open, Inactive expanded and scrolled; the result is noted in the PR.
- [ ] Opening: with 50 Forwards, opening the panel takes no more than 100 ms on the main thread (Instruments, Time Profiler; figure noted in the PR); no core call in the opening path.
- [ ] No interface text contains a word forbidden by `GLOSSARY.md`, nor "on"/"off" as a switch state (search test over the Swift files and string catalogs: tunnel, status, failure, port conflict, active pod, on/off; French: tunnel, statut, échec, conflit de port, doublon, pod actif, on/off).

### Open questions

Closed by the user's decisions: the click-on-row detail (decided), the 10-Forward threshold and the search/filter reset (no search or filter any more), the 440 pt list height (decided, and the scroll is kept when it is reached).

1. **Reconnecting on the first connection.** Activating a Forward first shows `Reconnecting` (reason "looking for a Ready pod"), and with the master switch several rows do it at once. The glossary defines Reconnecting as following a loss. Options: (a) accept it, the orange chip and the reason are enough; (b) add a "Connecting" state to the glossary and to `RealStateView`. Recommendation: (a) for v0.2, and ask Dodie about the glossary.
2. **Changing the kubeconfig with Active Forwards**: do they restart? Not visible in the interface. If yes, add "Active Forwards will restart." (« Les Forwards Actifs seront relancés. ») under the field; business rule for Dodie.
3. **Is a newly created Forward Active or Inactive?** The code saves without activating. An "Activate after saving" checkbox is possible; for the owner to decide.
4. **⌘N, ⌘R, ⌘, ⌘Q** work only while the panel has focus, which is the case on opening. To confirm on render that they do not clash with system shortcuts.
5. **Idle timeout is not in `GLOSSARY.md`** although the core (`idle_timeout`, « Délai d'inactivité ») and the strings on main use it. The spec uses "Idle timeout" for the concept and "Auto-deactivate" for the label, as main does. Dodie to add the term.
6. **Chip contrast.** A white glyph on system orange is probably under 3:1. To check on render; if it fails, a darker glyph on the orange chip (still a system colour).
7. **Master switch memory across relaunch**: I persist the remembered set in `UserDefaults` (decision made in this spec, to confirm). The alternative is memory only, which forgets it when the app quits.
8. **Quit Kubeport is two clicks deep** (Settings screen), by the user's choice; ⌘Q works while the panel is focused.

---

## 5. Prioritized changes

Size estimate: S (< ½ day), M (≈ 1 day), L (several days).

### Must: do before showing the app to others

| # | Change | Files | Size | Core / FFI |
|---|---|---|---|---|
| M1 | `hasLoaded` flag: "Loading Forwards…" instead of the false empty state; master switch and "New Forward…" greyed out while false | `CoreModel.swift`, `PanelView.swift` | S | no |
| M2 | Useful startup error: "Open Config" and "Try Again" work without a core; Settings reachable without a core | `CoreModel.swift`, `PanelView.swift`, `SettingsView.swift` | S | **optional**: expose `default_config_path()` as a free FFI function. Without it, duplicate the path in Swift (risk of drifting from `Config::default_path`) |
| M3 | Wi-Fi panel shell: 304 pt, title row with master switch, footer rows "New Forward…" / "Kubeport Settings…", one scroll area ≤ 440 pt with a visible indicator, `LazyVStack` with pinned headers | `PanelView.swift` | M | no |
| M4 | Master switch: remembered set (`UserDefaults`), derived on/off value, greyed-out rule, VoiceOver label and hint | `PanelView.swift`, `CoreModel.swift` | M | no (re-activation loops over the existing `activate`) |
| M5 | Active section and "Inactive Forwards ›" disclosure (count, remembered expansion), both grouped by Context · namespace; the Active/Inactive split computed in `CoreModel` | `PanelView.swift`, `CoreModel.swift` | M | no |
| M6 | Row: 28 / 24 pt chips with distinct glyph shapes, reason under the name, small named switch on every row, state never by colour alone | `PanelView.swift` (`ForwardRow`) | S | no |
| M7 | Inline detail (one at a time): Target, Resolved pod, Address + Copy, Idle timeout countdown (60 s refresh only while open), Edit…, Duplicate…, Try Again on Error; VoiceOver actions; right-click kept | `PanelView.swift`, `CoreModel.swift` (`retry(_:)`) | M | no (the idle deadline is already in the status on main) |
| M8 | Notice at the `PanelView` level (all screens), closable, announced to VoiceOver, adapted duration | `PanelView.swift`, `CoreModel.swift` | S | no |
| M9 | Form: per-field loading, stale responses ignored, 10 s timeout, inline messages, No Context / No Target / Context missing cases | `ForwardForm.swift` | M | **recommended**: a timeout in `catalog::namespaces/targets`. Without it, a timeout race on the Swift side (`withTaskGroup`) |
| M10 | Deletion: separate confirmation line, focus on Cancel, closes after 5 s | `ForwardForm.swift` | S | no |

### Should: right after

| # | Change | Files | Size | Core / FFI |
|---|---|---|---|---|
| S1 | Settings: immediate launch switch, Kubeconfig "Apply" and "Choose…", System Settings link, version, and the "Open Config" / "Reload Config" / "Quit Kubeport" rows | `SettingsView.swift` | S | no |
| S2 | Keyboard navigation (↑ ↓ Space Return, → ← on the disclosure row, ⌘N ⌘, ⌘R ⌘Q, Esc), nothing focused on opening | `PanelView.swift` | M | no (macOS 14: `onKeyPress`) |
| S3 | Back to the list and no open detail on each opening of the panel | `PanelView.swift` | S | no (to check: `onAppear` is not called again on each opening with `.window`; observe `NSWindow.didBecomeKeyNotification`) |
| S4 | Form: inline validation (name taken, port 1–65535), Competing Forwards information, "will restart" warning | `ForwardForm.swift` | S | no |
| S5 | Icon: dynamic `accessibilityDescription`; check the Error state on light and dark bars, template fallback if unreadable | `StatusIcon.swift`, `KubeportApp.swift` | S | no |
| S6 | "Competing Forward" sentence in the detail of a row whose local port is shared | `PanelView.swift` | S | no |
| S7 | Row move between sections: 0.2 s animation (off with "Reduce motion"), VoiceOver announcement, focus follows the row | `PanelView.swift` | S | no |

### Could: later

| # | Change | Core / FFI |
|---|---|---|
| C1 | "Config reloaded: N Forwards." notice after Reload | **Yes**: `Command::Reload` only reports failure |
| C2 | Notice severity (information / error), to keep errors until closed | **Yes**: `notice(message)` is a bare string |
| C3 | A "Connecting" state distinct from "Reconnecting" | **Yes**: `RealStateView` and glossary (question 1) |
| C4 | Pre-check "local port already in use by another process" in the form | **Yes** (or Swift via a trial `bind`) |
| C5 | Shorten long Context names on display (`arn:aws:eks:…/prod` → `prod`, full name in a tooltip) | no |
| C6 | Group action (deactivate the group's Forwards) in a context menu on the group header | no |
| C7 | Namespace or Target picker with search in the form (clusters with hundreds of namespaces) | no |

Dropped with the first proposal: search and state filter, collapsible groups, header summary, `⋯` menu, "Deactivate All" with Undo (the master switch replaces it), "Try Again for All".

### What these changes do not cost

No added dependency, no webview, no polling, no timer at idle: consistent with ADRs 0002 and 0003. The only added data are derived values in memory and two `UserDefaults` entries (the Inactive section's expanded flag and the master switch's remembered names). No core change is mandatory: Could items C1 to C4 need one, and M2 and M9 have a pure-Swift alternative.
