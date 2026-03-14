function fizzbuzz(n: integer): text
  intent: return fizz buzz or the number as text
  if n % 15 == 0 then
    "FizzBuzz"
  else if n % 3 == 0 then
    "Fizz"
  else if n % 5 == 0 then
    "Buzz"
  else
    to_text(n)
  end
end

function main(): nothing
  intent: print fizzbuzz for numbers 1 through 20
  for i in range(1, 21) do
    print(fizzbuzz(i))
  end
end
