import { describe, expect, it } from "vitest";
import { joinAddress, splitAddress } from "./address";

describe("address", () => {
  it("cuts the stored text into street, postal code and city", () => {
    expect(splitAddress("190 rue Pierre et Marie Curie\n60280 Margny-lès-Compiègne")).toEqual({
      street: "190 rue Pierre et Marie Curie",
      postalCode: "60280",
      city: "Margny-lès-Compiègne",
    });
  });

  it("keeps a text without postal code as the street", () => {
    expect(splitAddress("12 rue des Lilas")).toEqual({
      street: "12 rue des Lilas",
      postalCode: "",
      city: "",
    });
    expect(splitAddress(null)).toEqual({ street: "", postalCode: "", city: "" });
  });

  it("puts the parts back, and nothing when they are empty", () => {
    const parts = { street: "12 rue des Lilas", postalCode: "75011", city: "Paris" };
    expect(joinAddress(parts)).toBe("12 rue des Lilas\n75011 Paris");
    expect(splitAddress(joinAddress(parts))).toEqual(parts);
    expect(joinAddress({ street: " ", postalCode: "", city: "" })).toBeNull();
  });
});
