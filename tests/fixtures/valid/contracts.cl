record Account
  owner: text
  balance: decimal
end

function deposit(account: Account, amount: decimal): Account
  intent: add amount to account balance
  requires: amount > 0.0
  ensures: result.balance == account.balance + amount
  account with { balance: account.balance + amount }
end

function withdraw(account: Account, amount: decimal): Account
  intent: subtract amount from account balance safely
  requires: amount > 0.0, account.balance >= amount
  ensures: result.balance == account.balance - amount
  account with { balance: account.balance - amount }
end

function main(): nothing
  intent: demonstrate deposit and withdrawal with contracts
  let acc: Account = Account { owner: "Alice", balance: 100.0 }
  let acc2: Account = deposit(acc, 50.0)
  print("After deposit: " ++ to_text(acc2.balance))
  let acc3: Account = withdraw(acc2, 30.0)
  print("After withdraw: " ++ to_text(acc3.balance))
end
