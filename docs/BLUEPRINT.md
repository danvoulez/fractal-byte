# UBL — Universal Business Ledger (Blueprint)

Tudo é **chip**, exceto o **gate**. O **recibo** é a imagem/persistência/trajetória.

Camadas do fractal:
- **Byte (ai‑nrf1)**: 1 byte → 1 mensagem → 1 CID. NRF‑1.1, zero‑choice.
- **Semântica (ubl‑chip)**: envelope determinístico dos dados.
- **Política (ubl‑policy via recibos)**: decisões (ALLOW/DENY) + evidências.

Princípios LLM‑first:
- Ghost records, write‑before/after execute, trilha de auditoria por receipts, privacidade por cápsulas.

Identidade/endereçamento:
- CIDv1 (raw/sha2‑256), URL por CID, DID: `did:cid:<cid>` e DID do runtime (`did:key`/`did:web`).

Componentes:
- **ubl_gate** (REST): ingest, resolve, get cid, receipts.
- **ubl_ai_nrf1**: encoder/decoder NRF‑1.1 e CID.
- **ubl_ledger**: armazenamento conteúdo‑endereçado (FS; pronto p/ S3).
- **ubl_receipt**: emissão e leitura de recibos (JWS Ed25519; PQ placeholder).
- **ubl_did**: doc do runtime e resolução de did:cid.
- **ubl_config**: BASE_URL e parâmetros.

Operação mínima: `make run` em dev, Docker em prod. Segurança: mover chaves para KMS/HSM; attestation real depois.
