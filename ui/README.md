# UBL Receipt Registry UI

A modern React web app for browsing and inspecting UBL canonical receipts.

## Features

- **Receipt explorer** — browse all receipts (WA, Transition, WF) with expandable cards
- **Decision badges** — visual ALLOW/DENY/WARN indicators with color coding
- **Policy trace** — inline display of cascade policy evaluation trace
- **CID navigation** — click parent CIDs to search related receipts
- **Filters** — search by CID/content, filter by type and decision
- **Stats dashboard** — total receipts, write-final count, deny count
- **Ghost mode** — visual indicator for ghost receipts
- **Live health** — gate server status and version display

## Development

```bash
cd ui
npm install
npm run dev
```

The dev server proxies `/v1/*` requests to `http://localhost:3000` (the UBL Gate).

## Production Build

```bash
npm run build
```

Output goes to `ui/dist/`.

## Tech Stack

- **React 18** + TypeScript
- **Vite** for bundling
- **TailwindCSS** for styling
- **Lucide React** for icons
- **Inter** + **JetBrains Mono** fonts
