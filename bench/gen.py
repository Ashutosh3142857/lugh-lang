"""Generate deterministic benchmark data for Lugh runtimes."""
import json, os, random

random.seed(42)
os.makedirs("bench/data", exist_ok=True)
clients = [f"Client{i:04d}" for i in range(400)]
invoices = [{"id": f"INV-{i}", "client": random.choice(clients), "amount": random.randrange(1000, 90000, 500)} for i in range(1000)]
payments = [{"ref": f"UTR{i}", "payer": random.choice(clients), "amount": random.randrange(1000, 90000, 500)} for i in range(1000)]
words = "invoice payment overdue meeting lunch report deadline contract renewal travel hotel standup budget review offer launch".split()
emails = [{"subject": " ".join(random.choices(words, k=4)), "body": " ".join(random.choices(words, k=40)) + "."} for i in range(3000)]
for name, data in [("invoices", invoices), ("payments", payments), ("emails", emails)]:
    json.dump(data, open(f"bench/data/{name}.json", "w"))
print("bench data written")
