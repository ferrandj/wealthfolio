import { describe, expect, it } from "vitest";
import { isJointHolder, matchesHolderFilter } from "./holders";

describe("joint accounts", () => {
  it("recognises two holders joined by a conjunction", () => {
    expect(isJointHolder("M DUPONT JEAN ET MME DUPONT MARIE")).toBe(true);
    expect(isJointHolder("MR JEAN DUPONT OU MME MARIE DUPONT")).toBe(true);
    expect(isJointHolder("M DUPONT JEAN MME DUPONT MARIE")).toBe(true);
  });

  it("leaves an individual account alone", () => {
    expect(isJointHolder("M DUPONT JEAN")).toBe(false);
    expect(isJointHolder("")).toBe(false);
    expect(isJointHolder(null)).toBe(false);
  });

  it("filters", () => {
    expect(matchesHolderFilter("M A ET MME B", "joint")).toBe(true);
    expect(matchesHolderFilter("M A ET MME B", "individual")).toBe(false);
    expect(matchesHolderFilter("M A", "individual")).toBe(true);
    expect(matchesHolderFilter("M A", "all")).toBe(true);
  });
});
