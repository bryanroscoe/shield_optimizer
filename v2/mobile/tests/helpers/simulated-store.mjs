export class SimulatedStore {
  constructor(emit) { this.emit = emit; }
  begin(request, operation = "buy") { this.emit({ type: "begin", request, operation }); }
  respond(request, type) {
    if (!["pending", "purchased", "canceled", "error"].includes(type)) throw new Error("Store cannot verify entitlement");
    this.emit({ type, request });
  }
}
