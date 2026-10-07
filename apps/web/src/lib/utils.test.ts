import { describe, expect, it } from "vitest"

import { cn } from "./utils"

describe("cn", () => {
  it("combines conditional and nested class inputs", () => {
    expect(cn("flex", ["items-center", false], { hidden: false, block: true }))
      .toBe("items-center block")
  })

  it("lets a component caller override conflicting Tailwind utilities", () => {
    expect(cn("px-2 py-1 text-sm", "px-4 text-lg"))
      .toBe("py-1 px-4 text-lg")
  })

  it("keeps responsive and interaction variants separate", () => {
    expect(cn("p-2 hover:bg-red-500 md:p-4", "p-3 hover:bg-blue-500"))
      .toBe("md:p-4 p-3 hover:bg-blue-500")
  })
})
