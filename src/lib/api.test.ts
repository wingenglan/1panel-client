import { describe, expect, it } from "vitest";
import { isHostKeyChallenge } from "./api";

describe("connection outcomes", () => {
  it("distinguishes host key challenges from connection snapshots", () => {
    expect(isHostKeyChallenge({ serverId: "a", host: "host", port: 22, keyType: "ssh-ed25519", fingerprint: "SHA256:test" })).toBe(true);
    expect(isHostKeyChallenge({ serverId: "a", status: "online", connectedAt: null, error: null })).toBe(false);
  });
});
