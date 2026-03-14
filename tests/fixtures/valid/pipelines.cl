record Person
  name: text
  age: integer
end

function get_senior_names(people: a list of Person): a list of text
  intent: filter people older than 65 and return their names sorted
  people
    |> filter(fn(p: Person): boolean => p.age > 65)
    |> sort_by(fn(p: Person): text => p.name)
    |> map(fn(p: Person): text => p.name)
end

function main(): nothing
  intent: demonstrate pipeline processing on a list of people
  let people: a list of Person = [
    Person { name: "Alice", age: 70 },
    Person { name: "Bob", age: 45 },
    Person { name: "Carol", age: 68 }
  ]
  let names: a list of text = get_senior_names(people)
  for name in names do
    print(name)
  end
end
