/**
 * Le client décide sur le CODE stable du démon, plus sur la phrase (B2).
 */
import { describe, expect, test } from "bun:test";
import { messageDeCode } from "../src/erreurs-demon.ts";

describe("messageDeCode", () => {
  test("un code connu explique, la phrase du démon suit comme cause", () => {
    expect(messageDeCode("agent_exited", "send not confirmed: upstream finished: t1")).toBe(
      "The agent has exited\nCause: send not confirmed: upstream finished: t1",
    );
    expect(messageDeCode("busy", "")).toBe(
      "Busy: other actions are queued on this agent. Retry in a moment",
    );
  });

  test("un code inconnu ou absent : la phrase, traduite si on la reconnaît", () => {
    expect(messageDeCode("un_code_neuf", "something odd")).toBe("something odd");
    expect(messageDeCode(undefined, "this project no longer exists")).toBe(
      "This project no longer exists in Herdr\nCause: this project no longer exists",
    );
  });
});
