// The display transport (RFC 005 rule 14): one reader interface over two implementations.
// The Host's worker writes each batch of sixteen-byte records through a writer; the page reads
// once per animation frame through a reader that does not know which it reads from.
//
// Shared: a single-producer single-consumer ring in one SharedArrayBuffer, where the page is
// cross-origin isolated. A header of eight Int32 (head, tail, dropped, capacity in records, the
// stream's version, three reserved), then the records. Head and tail are free-running counters; a record's index is the counter
// masked by capacity - 1. The writer stores head after the bytes; the reader stores tail after
// the copy; a record that does not fit is counted as dropped and never written.
//
// Messages: one transferred buffer per batch, its first word the stream's version (a little-endian
// u32) and then the records, queued on the page with a bound of 64; when the page falls behind,
// the oldest buffer goes and its records are counted as dropped.
//
// The version (RFC 014 rule 3): 2 in this release. A reader of a newer stream refuses it before
// it reads a record (`refused` holds the version found, and `read()` gives nothing); the layout of
// a record and the kinds a reader does not know are the Host's (`florn-control`, `display`).
//
// Both deliver every record while the reader keeps up; what is shown is a reduction above
// this module, the same for both (the view model's). The layout of the records themselves is
// the Host's (`florn-control`, `display`).
export const RECORD = 16;
export const RING_RECORDS = 65536;
export const RING_HEADER = 32;
export const QUEUE_BOUND = 64;
export const VERSION = 2;
/// The word before the records of a message batch.
export const BATCH_WORD = 4;

/// Whether a reader of VERSION may read a stream of `found`: older or equal, yes.
export function checkVersion(found) {
  return found <= VERSION;
}

/// The ring's buffer, for the page to make and hand to the worker.
export function createRing(version = VERSION) {
  const sab = new SharedArrayBuffer(RING_HEADER + RING_RECORDS * RECORD);
  const header = new Int32Array(sab, 0, 8);
  Atomics.store(header, 3, RING_RECORDS);
  Atomics.store(header, 4, version);
  return sab;
}

export class RingWriter {
  constructor(sab) {
    this.header = new Int32Array(sab, 0, 8);
    this.data = new Uint8Array(sab, RING_HEADER);
    this.capacity = Atomics.load(this.header, 3);
    this.kind = "ring";
  }
  /// Write what fits of `bytes` (a multiple of RECORD); the records that did not fit are
  /// counted in the header and returned.
  write(bytes) {
    const n = bytes.length / RECORD;
    const head = Atomics.load(this.header, 0) >>> 0;
    const tail = Atomics.load(this.header, 1) >>> 0;
    const used = (head - tail) >>> 0;
    const free = this.capacity - used;
    const write = Math.min(n, free);
    const dropped = n - write;
    if (dropped > 0) Atomics.add(this.header, 2, dropped);
    if (write === 0) return dropped;
    const mask = this.capacity - 1;
    const start = (head & mask) * RECORD;
    const first = Math.min(write * RECORD, this.data.length - start);
    this.data.set(bytes.subarray(0, first), start);
    if (first < write * RECORD) this.data.set(bytes.subarray(first, write * RECORD), 0);
    Atomics.store(this.header, 0, (head + write) | 0);
    return dropped;
  }
}

export class RingReader {
  constructor(sab) {
    this.header = new Int32Array(sab, 0, 8);
    this.data = new Uint8Array(sab, RING_HEADER);
    this.capacity = Atomics.load(this.header, 3);
    this.kind = "ring";
    /// The version the stream declares, and the same when it is newer than this reader's: the
    /// stream is refused before a record is read.
    this.version = Atomics.load(this.header, 4);
    this.refused = checkVersion(this.version) ? null : this.version;
  }
  /// Every record written since the last read, as one copy; empty when there is none.
  read() {
    if (this.refused !== null) return new Uint8Array(0);
    const head = Atomics.load(this.header, 0) >>> 0;
    const tail = Atomics.load(this.header, 1) >>> 0;
    const avail = (head - tail) >>> 0;
    if (avail === 0) return new Uint8Array(0);
    const mask = this.capacity - 1;
    const start = (tail & mask) * RECORD;
    const out = new Uint8Array(avail * RECORD);
    const first = Math.min(avail * RECORD, this.data.length - start);
    out.set(this.data.subarray(start, start + first), 0);
    if (first < avail * RECORD) out.set(this.data.subarray(0, avail * RECORD - first), first);
    Atomics.store(this.header, 1, head | 0);
    return out;
  }
  /// Records the writer could not fit, in all.
  dropped() {
    return Atomics.load(this.header, 2);
  }
}

export class QueueWriter {
  constructor(post, version = VERSION) {
    this.post = post;
    this.version = version;
    this.kind = "messages";
  }
  /// One transferred message per batch, the version first; the sender never drops.
  write(bytes) {
    const copy = new Uint8Array(BATCH_WORD + bytes.length);
    new DataView(copy.buffer).setUint32(0, this.version, true);
    copy.set(bytes, BATCH_WORD);
    this.post({ type: "display", buffer: copy.buffer }, [copy.buffer]);
    return 0;
  }
}

export class QueueReader {
  constructor() {
    this.pending = [];
    this.lost = 0;
    this.kind = "messages";
    this.refused = null;
    /// The version of the last batch that arrived.
    this.version = null;
  }
  /// A batch arrived; its first word is the stream's version, and a newer one is refused before a
  /// record is read (nothing of it is kept, and what came before stays). Beyond the bound the
  /// oldest goes and its records count as dropped.
  push(buffer) {
    if (buffer.byteLength < BATCH_WORD || (buffer.byteLength - BATCH_WORD) % RECORD !== 0) return 0;
    const found = new DataView(buffer).getUint32(0, true);
    this.version = found;
    if (!checkVersion(found)) {
      this.refused = found;
      return 0;
    }
    this.pending.push(new Uint8Array(buffer, BATCH_WORD));
    if (this.pending.length > QUEUE_BOUND) {
      const old = this.pending.shift();
      this.lost += old.length / RECORD;
      return old.length / RECORD;
    }
    return 0;
  }
  read() {
    if (this.pending.length === 0) return new Uint8Array(0);
    if (this.pending.length === 1) return this.pending.pop();
    let total = 0;
    for (const b of this.pending) total += b.length;
    const out = new Uint8Array(total);
    let at = 0;
    for (const b of this.pending) {
      out.set(b, at);
      at += b.length;
    }
    this.pending.length = 0;
    return out;
  }
  dropped() {
    return this.lost;
  }
}

/// The reader for this page: the ring where the page is isolated, messages otherwise.
export function createReader() {
  if (self.crossOriginIsolated === true && typeof SharedArrayBuffer === "function") {
    const sab = createRing();
    return { reader: new RingReader(sab), ring: sab, transport: "shared" };
  }
  return { reader: new QueueReader(), ring: null, transport: "messages" };
}

/// The writer for the worker: over the ring the page made, or messages through `post`.
export function createWriter(ring, post) {
  return ring ? new RingWriter(ring) : new QueueWriter(post);
}

/// Count a byte string's records by kind: instants (batches), flights, activity, samples.
export function count(bytes, into) {
  for (let i = 0; i < bytes.length; i += RECORD) {
    const kind = bytes[i];
    if (kind === 0) into.instants += 1;
    else if (kind === 1) into.flights += 1;
    else if (kind === 2) into.activity += 1;
    else if (kind === 3) into.samples = (into.samples || 0) + 1;
    else into.unknown += 1;
  }
  into.records += bytes.length / RECORD;
  return into;
}

/// What a reader does with a stream of an older, the same and a newer version, over each
/// transport this page has, from batches of four records (an instant, a kind the reader does not
/// know, a sample, an instant): the conformance kit's check (RFC 014 rule 3).
export function probeVersions() {
  const batch = new Uint8Array(4 * RECORD);
  [0, 200, 3, 0].forEach((kind, i) => {
    batch[i * RECORD] = kind;
  });
  const out = { version: VERSION, queue: {}, ring: null };
  for (const [name, version] of [["older", 1], ["same", VERSION], ["newer", VERSION + 1]]) {
    const reader = new QueueReader();
    const writer = new QueueWriter((msg) => reader.push(msg.buffer), version);
    writer.write(batch);
    const bytes = reader.read();
    out.queue[name] = { records: bytes.length / RECORD, refused: reader.refused, version: reader.version };
  }
  if (typeof SharedArrayBuffer === "function" && self.crossOriginIsolated === true) {
    out.ring = {};
    for (const [name, version] of [["older", 1], ["same", VERSION], ["newer", VERSION + 1]]) {
      const sab = createRing(version);
      new RingWriter(sab).write(batch);
      const reader = new RingReader(sab);
      const bytes = reader.read();
      out.ring[name] = { records: bytes.length / RECORD, refused: reader.refused, version: reader.version };
    }
  }
  return out;
}
