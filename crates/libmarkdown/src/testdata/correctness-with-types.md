---
title: Aiming for correctness with types
date: "2020-12-12T00:30:00Z"
tags:
  - javascript
  - typescript
  - golang
  - rust
extra:
  patreon: true
---

The [Nature weekly journal of
science](https://www.nature.com/nature-research/about) was first published in 1869. And after one and a half century, it has finally completed one cycle of
[carcinization](https://en.wikipedia.org/wiki/Carcinisation), by publishing
an article about [the Rust programming language](https://www.rust-lang.org/).

It's a [really good article](https://www.nature.com/articles/d41586-020-03382-2).

What I liked about this article is that it didn't _just_ talk about
performance, or even just memory safety - it also talked about correctness.

Well, it also talked about diversity and inclusion, which I think is also
extremely important, but it's not an _intrinsic_ quality of the language,
more of a state of affairs - which we cannot take for granted, as the nature
of human dynamics is that they are... dynamic.

{% sc amossays %}
Which is not to say that the quality of the community around Rust, those who
build, use, and teach Rust, does not affect the quality of the language
itself. Quite the contrary. What I _am_ saying, is that if we are not
careful, a community can rapidly degrade, especially as a language gains
wider adoption.
{% endsc %}

{% sc bearsays %}
Right! It's not quite as simple as "one bad apple spoils the bunch", it's
more about eventual moderator burnout.

I mean, if you just take a look at Re-
{% endsc %}

{% sc amossays %}
Uhhh moving on
{% endsc %}

{% sc bearsays %}
...fine.
{% endsc %}

With all that said - I don't feel especially qualified to discuss that topic
at length right now (possibly ever), which is why for today, I'll try to
remain focused on the notion of correctness.

## The challenges of Rust advocacy

Whenever the topic of Rust comes up, it's usually in comparison with some
other language. And quite often (to the chagrin of many in the community),
the conversation devolves into a series of arguments about why some piece of
software ought to be written (or rewritten) in Rust.

This pattern is so common, it has become a meme - with its own acronym: RIIR,
for "Rewrite It In Rust". If you put those words in a search engine you'll
find no shortage of articles explaining why you should - or shouldn't - RIIR.

But apart from their frequency and length, there is something else that's
extremely common about these arguments. The "do RIIR" side, despite their
best efforts, is frequently perceived by the other side as being "superior"
or "elitist".

This is made worse by articles in the style of "I tried to RIIR, and it
didn't work out for me", which usually leads to one of several conclusions,
some of which are: "the promises made by Rust were not upheld", or "the
author went about this all wrong", neither of which are particularly good
press for, well, Rust.

I've tried to pinpoint what exactly about Rust "evangelism" makes it seem so
unpalatable to folks who are perfectly comfortable using the languages
they've been using for years (sometimes decades), and I've come to an
explanation I'm reasonably happy with.

It comes in the form of a collection of statements, all of which I believe
are true simultaneously:

**1) Programming in Rust requires you to think differently**

This has several implications: first, trying to replicate patterns that are
common in other languages is often bound to fail spectacularly. This makes
the learning experience quite frustrating for some, and is in itself enough
to explain why a lot of the "I tried to RIIR" articles end up the way they
do.

To an "outsider" (someone who has never written Rust), this statement alone
also _already_ feels superior. If you've gone through the wonderful
experience of getting a new manager who feels like they need to change
everything slightly just to assert their position - this is what it _can_
feel like.

{% sc amossays %}
That feeling tends to dissipate after persevering for a period of time. What
once appeared as petty "calls to authority", changes for changes' sake, are
eventually almost all revealed to be _fundamental_ changes, that are
necessary to make the whole system work.

And sometimes they're just current limitations of the language and/or its
implementations. That's something the C++ crowd runs into a lot more.
{% endsc %}

{% sc bearsays %}
Wait, implementations, plural? I thought Rust had no spec and there was only
one compiler?
{% endsc %}

{% sc amossays %}
Arguably, [rust-analyzer](https://rust-analyzer.github.io/) is a partial
reimplementation of a lot of the language. Inside rustc itself, there are
several concurrent implementations of the same components.

See for example [Polonius](https://github.com/rust-lang/polonius), the
next-generation borrow checker, the [Miri](https://github.com/rust-lang/miri)
interpreter, or the [Cranelift codegen
backend](https://github.com/bjorn3/rustc_codegen_cranelift).
{% endsc %}

This statement is equally irritating to the functional programming crowd, who
are _already_ enamored with languages that requires them to think
differently, sometimes much more differently, than "traditional" languages
like, C, C++, Java, Python, Go, etc.

{% sc amossays %}
"Traditional" is put in scare quotes here because of course, functional
programming languages are not particularly recent. I'm (mis)using it in the
sense "that you would find a lot of job openings for in the past decade".
{% endsc %}

"No", say the Haskellers, understandably, "Rust is _not_ a 'fundamental'
departure from '''traditional''' imperative programming languages, in fact,
look at it, and its filthy, filthy side effects".

To which I say: fair. But also: **the novelty is in the compromise**. If you
can find a way to reconcile two fundamentally different but well-established
methods, you've made something new, that solves a new category of problems,
or that solves more easily an old category of problems - at any rate: it's
worth looking into.

**2) It is harder to write any code at all in Rust**

Again, there are several ways to misconstrue this statement: I don't believe
Rust is particularly harder to write, than, say, x86 assembly.

{% sc bearsays %}
Or is it?
{% endsc %}

But it is, arguably, harder to write any code at all in Rust, than in Go, or
JavaScript. You can take a perfectly fine JavaScript program, and struggle
for _hours_ to rewrite it in Rust, because the compiler _requires_ you to
care about more aspects of the problem than you had to before.

Which begs the question: why would anyone submit themselves to this?

That is a completely fair question. Because in this instance, the JavaScript
program was "complete" before its Rust equivalent was, and they solved the
same problem. Sure, the Rust program may be faster - but is that enough of
a differentiator?

We could have shipped the JavaScript program earlier, acquired customers, and
increased revenue. And we could've worried about "minor bugs" and
"performance difficulties" later on.

Or so I'm being told.

And as Rust is beginning to see wider adoption, not only in almost all of the
major software companies, but in a lot of the smaller ones as well, this is
an argument that real actual people like you and me are going over several
times a day, every day of the week, because we as an industry are not good
with the whole work/life balance thingy.

**3) It is easier to write "correct" code in Rust**

This is where things get tricky.

Because "correct" is not an end goal. "Correct", much like chaos, is a ladder.

Unless you're embarking on a mission to the moon, or you're writing software
for self-driving cars, or... okay there's actually quite a few applications
for which you do need to be "correct" - but say, if you're working for a
company that sells a "non-essential" customer product or service (and that's
most of the industry), you only need to be _correct enough_.

Say you're writing a music recommendation system. The correctness
requirements here are _extremely lax_. You could totally get away with just
pulling from the "most listened" titles dataset, and vaguely bucketing things
by year or genres. Not that anyone would actually do that. Wink wink.

Point is: if 50% of your recommendations are only tangentially related to
your customer's interests, it probably won't hurt the bottom line. And
anyway: are they paying customers? Or are they just freeloaders making it up
by getting some ads shoved into their ears now and then? Maybe you could only
run the _real_ recommendation system for those premium accounts.

But I digress.

There's a _lot_ of software applications for which being correct is not all
that important. Unless customers - paying customers - start to notice, and
straight up threaten to leave you for a competitor if you don't fix the
incorrectness, presto.

Let's talk about uptime: the percentage of the time that a service is
"available", or "healthy". No one is foolish enough to promise 100% uptime.
We barely have enough control over matter to achieve `99.99%` uptime - and we
do so by building redundant systems. If a node won't handle requests
properly, just fall back to another node, or take it out of rotation, set up
more load balancers, filter out the word "latency" on Slack, do something,
anything!

And if you really _can't_ achieve the promised uptime, well, you still have a
way out: you can give the customer their money back, sort of, in the form of
a "credit", which effectively makes their next bill a little lighter.

But does that mean you shouldn't care, or worry about correctness? No!

{% sc bearsays %}
Today: Rhetorical questions 101, with Amos.
{% endsc %}

Every bit of incorrectness you ship has a cost. The most direct cost is
giving customers "credits" - you're literally taking a chunk out of your own
profits, as penance for failing to meet your own goals.

But fixing lots of "minor bugs" has an engineering cost, too. _Someone_ has
to go through the backlog, or the ice box, or wherever kids store their
~~plums~~ tickets nowadays, and actually ship the fix. And hope that their
fix does not introduce a regression.

So you write tests. And then some more tests. And some of them are flaky,
because of the law of large numbers, or something like that. So you allow
them to fail. And then you find bugs in your tests, so you fix those bugs
too, but not after you've "fixed" your code so it passes the tests,
introducing an error because it turns out the ~~call was coming from inside
the house~~ test was wrong all along.

And while your engineers are busy doing all this, they're not working on new
features. Features that would be much faster to build originally in Go, or
JavaScript, or so I'm told. And so your company falls behind, as others
continue to innovate, which could eventually cost you your entire
marketshare.

This is not a work of fiction - it's something that has happened in all
industries, for as long as we've had industries.

Of course, the reverse nightmare scenario is also real - we all know that one
colleague who, by our own estimation, spends "forever" trying to get
something juuuuuuuuust right. This can _also_ cause a company to fall behind
while others continue innovating and capturing the market.

So, as with a lot of things - it's a balance.

{% sc bearsays %}
Tonight, at 11: Platitudes, with Amos.
{% endsc %}

And if you've managed to not let yourself be distracted by the meanderings
the introduction to this article has taken, you might remember that I
mentioned Rust was a _compromise_, and so you may well have an inkling where
it is that I'm trying to go with all this.

And you would be correct.

{% sc bearsays %}
Ha!
{% endsc %}

## Implicit contracts are everywhere

The world is a messy, _messy_ place. Depending on how your brain apprehends
your surroundings, and your current mental state, the world can range from
"okay, I guess" to deeply upsetting.

Social interactions are a perfect way to familiarize ourselves with the
notion of "implicit contracts".

It is understood, among good company, that there are certain things one ought
not to discuss out loud. Or not with people you don't know well enough. Or
not with your family. Or not at all.

This is part of a "social contract", that I honestly don't remember signing,
which is kinda bullshit if you want my opinion, but regardless - a large
number of scholars agree that it is, indeed, "a thing", so let's just go with
it.

Kids in particular, tend to be frustrated by the vagueness of this social
contract. Kids, and [Nathan
Fielder](https://www.youtube.com/watch?v=I-67hbucUjQ), whose videos rarely
fail to make me laugh, but make others extremely uncomfortable, due to the
sheer awkwardness of not behaving like others expect you to, even in fairly
innocent situations.

The thing about this "social contract", apart from being poorly defined and
ever-evolving, is that there exists very little in the way of enforcing it.

Ah, to be an edgy teenager again, discovering - for the first time in
history, no doubt - the idea that "if we all stop going to school, there is
nothing they can do about it".

{% sc bearsays %}
I'm sure that went well.
{% endsc %}

{% sc amossays %}
Well, it wasn't as big a walkout as I had envisioned, but eventually
the school administration and I agreed that it was probably best if I
skipped certain classes for a while, so it all worked out in the end.
{% endsc %}

{% sc bearsays %}
I'm not sure you learned the right lesson from that, but discussing
incentives is probably best left for another day.
{% endsc %}

Anyway - the same "implicit contracts" apply to the tech world.

For instance, it is generally agreed-upon that hammering a server with
hundreds of thousands of requests in a short period of time is "rude".
But do it over it a period of ten years, and you're a "valued customer".

Confusing, I know.

And that's not all. If a service listens for TCP connections on port 80, it's
generally expected to speak
[HTTP](https://en.wikipedia.org/wiki/Hypertext_Transfer_Protocol). That one
is [actually codified in an RFC](https://tools.ietf.org/html/rfc1340), but
again, there's nothing preventing you from, you know, just not.

The rule is not _enforced_. Thankfully, there is no IETF police.

And as you gain customers, and your product is used by a wider variety of
folks, you tend to encounter more and more folks that "just" completely
disregard your assumptions.

Let's take one of my favorite examples and look at the SSH protocol: when
a client connects to an SSH server, one of the first thing that happens is
that the server sends its version to the client.

{% sc amossays %}
Why that part of the protocol exists, it's hard to say. Presumably, the
authors of SSH were eager to give potential attackers an easier way to test
for vulnerabilities simply by parsing the version string, as is the case
of the `Server` HTTP header.

Or maybe they didn't think of it that way. It's impossible to tell.
{% endsc %}

But wait, I lied! _Before_ the servers sends its version, it may send
"other lines of data".

Now, normally-behaved SSH servers usually send lines from a text file, and we
call this their "banner message". Or it can be automatically generated, and
then we call this a MOTD (for Message Of The Day).

But if you think outside the box... and you want to prevent attackers from
getting inside the box...

you can send...

"lines of data"...

very slowly...

forever.

This is called an [SSH Tarpit](https://nullprogram.com/blog/2019/03/22/), and
I think it's equal parts hilarious and brilliant.

It's also a clear violation of the implicit contract between an SSH client
and an SSH server. It's not the _only_ violation that can occur. For example,
the SSH server could just take a _very long_ time to accept the connection
(ie. to complete the [TCP handshake](https://developer.mozilla.org/en-US/docs/Glossary/TCP_handshake)).

But this violation is so common, it has ~~become a meme~~ caused all clients
to protect against it by default. Network applications tend to set "timeouts"
on operations - in this case, the "connect timeout" would expire, and the
client would simply give up, which would free it up to try again.

If the SSH server simply sent _nothing_, a "read timeout" might expire, and
again, the client would give up on this connection and try again.

In all four of the RFCs in which the SSH protocol is documented, the word
"timeout" is only mentioned once, to recommend that servers have an
[authentication timeout](https://tools.ietf.org/html/rfc4252#section-4).
There's no mention of connection timeouts, a testament to the fact that
it's just "one of these things you should know about if you program networked
applications".

Unfortunately, not all of "those things" are obvious, or even particularly
well-known.

{% sc recap %}
If I say "you can't talk to me like that", well, there's nothing preventing
you from continuing to talk to me like that. It's rude, but not impossible.

Software is rude _all the time_.
{% endsc %}

## Let's talk about HTTP headers

Imagine we have a server that speaks exclusively HTTP/1.1.

It serves a variety of domains, such as `internal.example.org`,
`ducks.example.org`, and `giraffes.example.org`.

The problem? You really _only_ want to serve `ducks.example.org` and
`giraffes.example.org` to everyone, while `internal.example.org` should only
be accessible from the company VPN.

HTTP/1.1 seems like a pretty simple protocol...

{% sc bearsays %}
...until you need to _actually_ implement it correctly, anyway - at that
point, all bets are off.
{% endsc %}

...so we may be tempted to just add a proxy that perform access control, by
parsing HTTP requests.

I'm sure we can cobble something together...

```javascript
// This code is full of sins - but it serves its purpose.

const net = require("net");

async function main() {
  let server = new net.Server({}, onConnection);
  server.on("error", (err) => {
    throw err;
  });
  let port = 8124;
  server.listen(port, () => {
    console.log(`Now listening on port ${port}`);
  });
}

function onConnection(sock) {
  (async () => {
    // Read a full HTTP/1.1 request
    let buf = "";
    while (true) {
      await readable(sock);
      buf += sock.read();

      if (buf.endsWith("\r\n\r\n")) {
        break;
      }
    }

    buf = buf.trim();
    console.log(`==== incoming HTTP request ====`);
    console.log(buf);
    console.log(`===============================`);
    console.log(`(came from ${JSON.stringify(sock.address())})`);
  })().catch((err) => {
    throw err;
  });
}

async function readable(r) {
  return new Promise((resolve, reject) => {
    r.once("readable", resolve);
    r.once("error", reject);
    r.once("close", reject);
  });
}

main().catch((err) => {
  throw err;
});
```

And run it:

```shell
$ node index.js
Now listening on port 8124
```

And then, from another shell:

```shell
$ domain="internal.example.org"; curl --connect-to "${domain}:80:localhost:8124" "http://${domain}"
```

{% sc tip %}
This works in bash or zsh - it sets a variable named `domain` to the value
`internal.example.org`, then instructs `curl` to _not_ perform a
[DNS](https://en.wikipedia.org/wiki/Domain_Name_System) lookup, but instead
connect directly to `localhost:8124`, which is the address our
[node.js](https://nodejs.org/) server listens on.
{% endsc %}

And our first shell session would show:

```shell
$ node index.js
Now listening on port 8124
==== incoming HTTP request ====
GET / HTTP/1.1
Host: internal.example.org
User-Agent: curl/7.73.0
Accept: */*
===============================
(came from {"address":"::ffff:127.0.0.1","family":"IPv6","port":8124})
```

What do we observe here?

The first line has the HTTP method, the path, and the protocol. All
subsequent lines (until `CRLFCRLF`) are for headers. If we want to filter by
host, we're going to want to parse those.

The result of `socket.address()` is sort of unexpected for me - I wasn't
planning on supporting IPv6, so let's try and disable that:

```javascript
// new: we specify a hostname of `0.0.0.0` (an IPv4 address)
server.listen(port, "0.0.0.0", () => {
  console.log(`Now listening on port ${port}`);
});
```

```shell
node index.js
Now listening on port 8124
==== incoming HTTP request ====
GET / HTTP/1.1
Host: internal.example.org
User-Agent: curl/7.73.0
Accept: */*
===============================
(came from {"address":"127.0.0.1","family":"IPv4","port":8124})
```

Okay, so - for the purposes of our exercise, let's assume that only the
following addresses can access the internal website:

- `127.0.0.x` (with any `x`)
- `2.58.12.x` (with any `x`)

So, we'll probably want a function that lets us know, given an IP address,
whether it's allowed to access the internal website or not.

```javascript
function isAllowed(addr) {
  return addr.startsWith("127.0.0.") || addr.startsWith("2.58.12.");
}
```

Then, we shall use it from a `handleRequest` function:

```javascript
async function handleRequest(sock, payload) {
  let { address } = sock.address();
  let status, output;

  if (isAllowed(address)) {
    status = "200 OK";
    output = "Access granted!";
  } else {
    status = "403 Forbidden";
    output = "Forbidden.";
  }

  console.log(`[${address}] ${status}`);
  sock.write(`HTTP/1.1 ${status}\r\n\r\n`);
  sock.write(`${output}\n`);
  sock.end();
}
```

And finally, change `onConnection` to use `handleRequest`:

```javascript
function onConnection(sock) {
  (async () => {
    // Read a full HTTP/1.1 request
    let buf = "";
    while (true) {
      await readable(sock);
      buf += sock.read();

      if (buf.endsWith("\r\n\r\n")) {
        break;
      }
    }

    buf = buf.trim();
    await handleRequest(sock, buf);
  })().catch((err) => {
    throw err;
  });
}
```

And, as we say in French, "le tour est joué"!

If we make a request to localhost, here `127.0.0.1` with IPv4, we get
a 200 OK:

```shell
$ domain="internal.example.org"; curl -v --connect-to "${domain}:80:localhost:8124" "http://${domain}"
* Connecting to hostname: localhost
* Connecting to port: 8124
*   Trying 127.0.0.1:8124...
* Connected to localhost (127.0.0.1) port 8124 (#0)
> GET / HTTP/1.1
> Host: internal.example.org
> User-Agent: curl/7.73.0
> Accept: */*
>
* Mark bundle as not supporting multiuse
< HTTP/1.1 200 OK
* no chunk, no close, no size. Assume close to signal end
<
Access granted!
* Closing connection 0
```

But if we make a request to our LAN IP (which is _not_ in the `192.168.x` here, because I
happen to be running all this on [WSL 2](https://en.wikipedia.org/wiki/Windows_Subsystem_for_Linux#WSL_2)),
we get a 403 Forbidden:

```shell
$ domain="internal.example.org"; curl -v --connect-to "${domain}:80:172.31.194.107:8124" "http://${domain}"
* Connecting to hostname: 172.31.194.107
* Connecting to port: 8124
*   Trying 172.31.194.107:8124...
* Connected to 172.31.194.107 (172.31.194.107) port 8124 (#0)
> GET / HTTP/1.1
> Host: internal.example.org
> User-Agent: curl/7.73.0
> Accept: */*
>
* Mark bundle as not supporting multiuse
< HTTP/1.1 403 Forbidden
* no chunk, no close, no size. Assume close to signal end
<
Forbidden.
* Closing connection 0
```

For completeness, here are the server logs:

```shell
$ node index.js
Now listening on port 8124
[127.0.0.1] 200 OK
[172.31.194.107] 403 Forbidden
```

Everything matches up. Wonderful.

Well, our program isn't quite complete yet - we _always_ apply access
control, even for public domains like `ducks.example.org`:

```shell
$ domain="ducks.example.org"; curl -I --connect-to "${domain}:80:172.31.194.107:8124" "http://${domain}"
HTTP/1.1 403 Forbidden
```

So, we need to _actually_ parse the incoming HTTP request.

Let's whip up something real quick:

```javascript
function parseRequest(payload) {
  let req = {
    headers: {},
  };

  let lines = payload.split(/[\r]?\n/);
  let tokens = lines.shift().split(" ");
  req.method = tokens[0];
  req.path = tokens[1];
  req.protocol = tokens[2];

  for (const line of lines) {
    let i = line.indexOf(":");
    let name = line.substring(0, i);
    let value = line.substring(i + 1).trim();
    req.headers[name] = value;
  }
  return req;
}
```

And use it from `handleRequest`:

```javascript
async function handleRequest(sock, payload) {
  // new:
  let req = parseRequest(payload);
  console.log(JSON.stringify(req, null, 2));

  // old:
  let { address } = sock.address();
  let status, output;

  if (isAllowed(address)) {
    status = "200 OK";
    output = "Access granted!";
  } else {
    status = "403 Forbidden";
    output = "Forbidden.";
  }

  console.log(`[${address}] ${status}`);
  sock.write(`HTTP/1.1 ${status}\r\n\r\n`);
  sock.write(`${output}\n`);
  sock.end();
}
```

Now our server logs are a lot more informative:

```shell
$ node index.js
Now listening on port 8124
{
  "headers": {
    "Host": "ducks.example.org",
    "User-Agent": "curl/7.73.0",
    "Accept": "*/*"
  },
  "method": "HEAD",
  "path": "/",
  "protocol": "HTTP/1.1"
}
[172.31.194.107] 403 Forbidden
```

Let's implement the rest of the logic we set out to implement:

```javascript
function isRestricted(req) {
  return req.headers.Host === "internal.example.org";
}
```

We can now change the condition in `handleRequest` to:

```javascript
if (!isRestricted(req) || isAllowed(address)) {
  status = "200 OK";
  output = "Access granted!";
} else {
  status = "403 Forbidden";
  output = "Forbidden.";
}
```

And everything behaves as expected.
[Allowlisted](https://www.urbandictionary.com/define.php?term=Allowlist) IPs
get access to everything, including the internal site:

```shell
$ for subdomain in ducks giraffes internal; do domain="${subdomain}.example.org"; echo $domain; curl -I --connect-to "${domain}:80:localhost:8124" "http://${domain}" ; done
ducks.example.org
HTTP/1.1 200 OK

giraffes.example.org
HTTP/1.1 200 OK

internal.example.org
HTTP/1.1 200 OK
```

Whereas other IP addresses get access only to the public sites:

```shell
$ for subdomain in ducks giraffes internal; do domain="${subdomain}.example.org"; echo $domain; curl -I --connect-to "${domain}:80:172.31.194.107:8124" "http://${domain}" ; done
ducks.example.org
HTTP/1.1 200 OK

giraffes.example.org
HTTP/1.1 200 OK

internal.example.org
HTTP/1.1 403 Forbidden
```

We're done! That was easy.

Well, except for one part. Our proxy isn't actually proxying anything at all.
For it to actually proxy anything, we need to... well, we need some server to
proxy to.

I know, we'll write it in Go! Because in the real world, the origin server
may be written by a completely different team, with different language
preferences.

```go
package main

import (
  "bufio"
  "fmt"
  "io"
  "log"
  "net"
  "strings"
)

const hostPrefix = "host: "

func main() {
  // This server is *not* meant to be exposed to the internet, so it only
  // binds to localhost, not `0.0.0.0`.
  addr := "localhost:8125"

  l, err := net.Listen("tcp4", addr)
  must(err)

  log.Printf("Now listening on %v", addr)

handleConn:
  for {
    conn, err := l.Accept()
    must(err)

    ip := strings.Split(conn.RemoteAddr().String(), ":")[0]
    log.Printf("Connection from %v", ip)

    buf := bufio.NewReader(conn)

    for {
      lineBytes, _, err := buf.ReadLine()
      line := strings.ToLower(string(lineBytes))
      log.Printf("%v", line)

      if strings.HasPrefix(line, hostPrefix) {
        host := strings.TrimPrefix(line, hostPrefix)
        switch host {
        case "ducks.example.org":
          reply(conn, "200 OK", "Have some happy ducks!")
        case "giraffes.example.org":
          reply(conn, "200 OK", "Here's a long neck")
        case "internal.example.org":
          reply(conn, "200 OK", "[CONFIDENTIAL] The secret ingredient is love")
        default:
          reply(conn, "404 Not Found", "No such domain is hosted on this server")
        }
        continue handleConn
      }
      must(err)
    }
  }
}

func reply(conn io.WriteCloser, status string, payload string) {
  fmt.Fprintf(conn, "HTTP/1.1 %s\r\n\r\n", status)
  fmt.Fprintf(conn, "%s\n", payload)
  conn.Close()
}

func must(err error) {
  if err != nil {
    log.Fatalf("%#v", err)
  }
}
```

```
$ go run main.go
2020/12/06 00:49:32 Now listening on localhost:8125
```

Our origin server is completely unprotected - but then again, it's not
exposed to the internet, so this is fine.

It works quite well, though!

```shell
$ for subdomain in ducks giraffes internal; do domain="${subdomain}.example.org"; echo "\n${domain}"; curl "http://${domain}" --connect-to "${domain}:80:localhost:8125" ; done

ducks.example.org
Have some happy ducks!

giraffes.example.org
Here's a long neck

internal.example.org
[CONFIDENTIAL] The secret ingredient is love
```

For the curious, here's the output from our Go server:

```shell
$ go run main.go
2020/12/06 00:51:54 Now listening on localhost:8125
2020/12/06 00:51:55 Connection from 127.0.0.1
2020/12/06 00:51:55 get / http/1.1
2020/12/06 00:51:55 host: ducks.example.org
2020/12/06 00:51:55 Connection from 127.0.0.1
2020/12/06 00:51:55 get / http/1.1
2020/12/06 00:51:55 host: giraffes.example.org
2020/12/06 00:51:55 Connection from 127.0.0.1
2020/12/06 00:51:55 get / http/1.1
2020/12/06 00:51:55 host: internal.example.org
```

So, the last missing piece of the puzzle is for the node.js "access control
proxy" to forward the request to the origin - and to forward the response
back to the client.

And here's one way we could do it:

```javascript
async function proxyRequest(sock, payload) {
  let originSock = await new Promise((resolve, reject) => {
    let sock = new net.Socket();
    sock.on("error", reject);
    sock.connect(8125, "127.0.0.1", () => {
      resolve(sock);
    });
  });
  originSock.write(payload);
  originSock.end();

  forward: while (true) {
    try {
      await readable(originSock);
    } catch (err) {
      break forward;
    }
    let buf = originSock.read();
    if (buf) {
      sock.write(buf);
    }
  }
  sock.end();
}
```

Which would seamlessly integrate with our existing server code, albeit
with the conditions flipped:

```javascript
async function handleRequest(sock, payload) {
  let req = parseRequest(payload);
  console.log(JSON.stringify(req, null, 2));

  let { address } = sock.address();

  if (isRestricted(req) && !isAllowed(address)) {
    let status = "403 Forbidden";
    let output = "Forbidden.";

    console.log(`[${address}] ${status}`);
    sock.write(`HTTP/1.1 ${status}\r\n\r\n`);
    sock.write(`${output}\n`);
    sock.end();
    return;
  }

  await proxyRequest(sock, payload);
}
```

And with that, doing requests to localhost (ie. from `127.0.0.1`) gives us
access to everything from the origin:

```shell
$ for subdomain in ducks giraffes internal; do domain="${subdomain}.example.org"; echo "\n${domain}"; curl --connect-to "${domain}:80:127.0.0.1:8124" "http://${domain}" ; done

ducks.example.org
Have some happy ducks!

giraffes.example.org
Here's a long neck

internal.example.org
[CONFIDENTIAL] The secret ingredient is love
```

And doing requests to the LAN IP address would _not_ give us access to
`internal.example.org` - just as we intended:

```shell
$ for subdomain in ducks giraffes internal; do domain="${subdomain}.example.org"; echo "\n${domain}"; curl --connect-to "${domain}:80:172.31.194.107:8124" "http://${domain}" ; done

ducks.example.org
Have some happy ducks!

giraffes.example.org
Here's a long neck

internal.example.org
Forbidden.
```

And there you have it. Our server infrastructure is feature complete. It does
serve all three sites, and in terms of access control, it even passes our
black-box test, where we use an external HTTP client to make a request and
only rely on the output.

Our solution however, has several flaws which are, as we're about to see,
quite problematic.

## HTTP is only as real as you want it to be

curl is a well-behaved citizen of the HTTP-verse.

By default, it sets the `Host` header to whatever was in the URL:

```shell
$ curl -v http://172.31.207.114:8124
*   Trying 172.31.207.114:8124...
* Connected to 172.31.207.114 (172.31.207.114) port 8124 (#0)
> GET / HTTP/1.1
> Host: 172.31.207.114:8124
> User-Agent: curl/7.73.0
> Accept: */*
>
* Mark bundle as not supporting multiuse
< HTTP/1.1 404 Not Found
* no chunk, no close, no size. Assume close to signal end
<
No such domain is hosted on this server
* Closing connection 0
```

And if you use the `-H` (or `--header`) flag to specify the `Host` header,
well, it replace it with that value:

```shell
$ curl -v http://172.31.207.114:8124 -H "Host: ducks.example.org"
*   Trying 172.31.207.114:8124...
* Connected to 172.31.207.114 (172.31.207.114) port 8124 (#0)
> GET / HTTP/1.1
> Host: ducks.example.org
> User-Agent: curl/7.73.0
> Accept: */*
>
* Mark bundle as not supporting multiuse
< HTTP/1.1 200 OK
* no chunk, no close, no size. Assume close to signal end
<
Have some happy ducks!
* Closing connection 0
```

If we use a different casing for the `Host` header, it normalizes it:

```shell
$ curl -v http://172.31.207.114:8124 -H "hoST: ducks.example.org"
*   Trying 172.31.207.114:8124...
* Connected to 172.31.207.114 (172.31.207.114) port 8124 (#0)
> GET / HTTP/1.1
> Host: ducks.example.org
> User-Agent: curl/7.73.0
> Accept: */*
>
* Mark bundle as not supporting multiuse
< HTTP/1.1 200 OK
* no chunk, no close, no size. Assume close to signal end
<
Have some happy ducks!
* Closing connection 0
```

And if we try to pass a _second_ `Host` header (even with a different
casing!), it protects us from ourselves, only setting the first one:

```shell
$ curl -v http://172.31.207.114:8124 -H "hoST: ducks.example.org" -H "HOst: giraffes.example.org"
*   Trying 172.31.207.114:8124...
* Connected to 172.31.207.114 (172.31.207.114) port 8124 (#0)
> GET / HTTP/1.1
> Host: ducks.example.org
> User-Agent: curl/7.73.0
> Accept: */*
>
* Mark bundle as not supporting multiuse
< HTTP/1.1 200 OK
* no chunk, no close, no size. Assume close to signal end
<
Have some happy ducks!
* Closing connection 0
```

But curl is not the only way we can make HTTP requests.

Let's handcraft an HTTP request. In the `evil-request.txt` file, we'll put:

```raw
GET / HTTP/1.1
Host: ducks.example.org
User-Agent: netcat/0.7.1

```

(The blank line at the end is important)

Of course I'm doing this from Linux, so it's only using `\n` as a line separator, and we
want `\r\n` in HTTP, so, with a little help from `sed`, we can fix that:

```shell
$ cat evil-request.txt | sed -z 's/\n/\r\n/g' | od -c
0000000   G   E   T       /       H   T   T   P   /   1   .   1  \r  \n
0000020   H   o   s   t   :       d   u   c   k   s   .   e   x   a   m
0000040   p   l   e   .   o   r   g  \r  \n   U   s   e   r   -   A   g
0000060   e   n   t   :       n   e   t   c   a   t   /   0   .   7   .
0000100   1  \r  \n  \r  \n
0000105
```

Okay, seems good! Let's use netcat to speak TCP to our node.js access control service:

```shell
$ cat evil-request.txt | sed -z 's/\n/\r\n/g' | nc 172.31.207.114 8124
HTTP/1.1 200 OK

Have some happy ducks!
```

Awesome. Who needs curl when you've got netcat?

{% sc bearsays %}
And who needs netcat when you've got [bash](https://falzon.me/en/post/burl-a-pure-bash-http-client/)??
{% endsc %}

Our request isn't really evil yet, though. Sure, recaptcha might look at it
sideways, because of the unusual [user agent](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/User-Agent).

We can make it a lot more evil... if we do this:

```raw
GET / HTTP/1.1
Host: internal.example.org
Host: ducks.example.org
User-Agent: netcat/0.7.1

```

```shell
$ cat evil-request.txt | sed -z 's/\n/\r\n/g' | nc 172.31.207.114 8124
HTTP/1.1 200 OK

[CONFIDENTIAL] The secret ingredient is love
```

Uh oh.

We're able to access the internal site from the outside! Our access control
is not controlling any access at all.

But what's actually happening here?

Well, here's what the logs for our node.js service show:

```raw
Now listening on port 8124
{
  "headers": {
    "Host": "ducks.example.org",
    "User-Agent": "netcat/0.7.1"
  },
  "method": "GET",
  "path": "HTTP/1.1"
}
```

And here's what the logs for our Go service show:

```raw
2020/12/07 21:22:25 Now listening on localhost:8125
2020/12/07 21:22:26 Connection from 127.0.0.1
2020/12/07 21:22:26 get http/1.1
2020/12/07 21:22:26 host: internal.example.org
```

The crux of the problem seems to be that they don't agree what the `Host`
should be.

The node.js service parses _all_ headers, and stores them in a JS object,
which for non-JS folks, is more or less a hashmap, except it's highly
optimized when there's a small number of keys (at least in V8 - I'm not sure
what happens elsewhere).

So when we parse this request:

```raw
GET / HTTP/1.1
Host: internal.example.org
Host: ducks.example.org
User-Agent: netcat/0.7.1
```

Our object first looks like this:

```json
{
  "Host": "internal.example.org"
}
```

And on the next line, it turns into this: `Host` is overwritten:

```json
{
  "Host": "ducks.example.org"
}
```

So, the node.js service _thinks_ we're requesting `ducks.example.org` and says:
door's open, come on in!

Our Go service, on the other hand, stops on the first `Host: ` header line it finds:

```go
    for {
      lineBytes, _, err := buf.ReadLine()
      line := strings.ToLower(string(lineBytes))
      log.Printf("%v", line)

      if strings.HasPrefix(line, hostPrefix) {
        host := strings.TrimPrefix(line, hostPrefix)
        // omitted: host handling goes here
        continue handleConn
      }
      must(err)
    }
```

So, the first `Host` line has `internal.example.org`, and that's what it
serves, not performing any further checks, since that's not its job!

But we can make an even _shorter_ evil request.

```raw
GET / HTTP/1.1
host: internal.example.org
User-Agent: netcat/0.7.1

```

(Again, the final blank line is significant).

```shell
$ cat evil-request.txt | sed -z 's/\n/\r\n/g' | nc 172.31.207.114 8124
HTTP/1.1 200 OK

[CONFIDENTIAL] The secret ingredient is love
```

{% sc bearsays %}
Right! Since the node.js service looks up the Host header in a case-sensitive
way, by doing `headers["Host"]`, it just gets `undefined`, because here the
Host header is, in fact, lowercase.
{% endsc %}

...whereas the Go service converts all header lines to lowercase before it
processes them:

```go
    for {
      lineBytes, _, err := buf.ReadLine()
      //                👇
      line := strings.ToLower(string(lineBytes))
      log.Printf("%v", line)

      // etc.
    }
```

## Where have all the good http packages gone?

And this is a good place to preempt some criticism: some of you may have paid
particularly close attention to the code _before_ I showed its flaws, and to
you, I say: well done!

Code review skills are important. And if you did, you may have seen this
_whole thing_ coming, before it unfolded. Double kudos.

More importantly, you may be thinking: Amos, that's silly. Nobody just
parses HTTP 1.1 like that, straight from the TCP firehose.

To which I say: bwahahahah. You sweet, sweet summer child. Yes they do. And
they [do it in C](https://www.youtube.com/watch?v=G7LJC9vJluU).

It's quite awful.

But more to the point - both node.js _and_ Go come with http packages, which
I carefully avoided... until now.

We're going to switch to using them, and hopefully fix that terrible, no good
security hole in the process. But here's the thing: I'm much less interested
in fixing that particular bug, than I am in **preventing that whole category
of bugs in the first place**.

That, to me, is the real prize. But we'll come back to that.

Let's start with Go. If we rewrite our origin server with Go, it might look a
little something like this:

```go
package main

import (
  "log"
  "net/http"
)

func main() {
  server := http.Server{
    Addr: "localhost:8125",
    Handler: http.HandlerFunc(func(rw http.ResponseWriter, r *http.Request) {
      switch r.Host {
      case "ducks.example.org":
        rw.Write([]byte("Have some happy ducks!\n"))
      case "giraffes.example.org":
        rw.Write([]byte("Here's a long neck\n"))
      case "internal.example.org":
        rw.Write([]byte("[CONFIDENTIAL] The secret ingredient is love\n"))
      default:
        rw.WriteHeader(404)
        rw.Write([]byte("No such domain is hosted on this server\n"))
      }
    }),
  }
  log.Printf("Will listen on %v", server.Addr)
  log.Fatalf("%+v", server.ListenAndServe())
}

func must(err error) {
  if err != nil {
    log.Fatalf("%#v", err)
  }
}
```

There's a lot of implicit behavior happening here. For example, if we look
up the documentation for `http.ResponseWriter.Write`, we learn the following:

> Write writes the data to the connection as part of an HTTP reply.

So far so good.

> If WriteHeader has not yet been called, Write calls
> `WriteHeader(http.StatusOK)` before writing the data.

I guess that _is_ the happy path.

> If the Header does not contain a `Content-Type` line, `Write` adds a
> `Content-Type` set to the result of passing the initial 512 bytes of written
> data to `DetectContentType`.

That's... opinionated.

Let's take a quick look at `DetectContentType`:

```go
// DetectContentType implements the algorithm described
// at https://mimesniff.spec.whatwg.org/ to determine the
// Content-Type of the given data. It considers at most the
// first 512 bytes of data. DetectContentType always returns
// a valid MIME type: if it cannot determine a more specific one, it
// returns "application/octet-stream".
func DetectContentType(data []byte) string {
  if len(data) > sniffLen {
    data = data[:sniffLen]
  }

  // Index of the first non-whitespace byte in data.
  firstNonWS := 0
  for ; firstNonWS < len(data) && isWS(data[firstNonWS]); firstNonWS++ {
  }

  for _, sig := range sniffSignatures {
    if ct := sig.match(data, firstNonWS); ct != "" {
      return ct
    }
  }

  return "application/octet-stream" // fallback
}
```

All the magic happens in the definition of `sniffSignatures` itself:

```go
// Data matching the table in section 6.
var sniffSignatures = []sniffSig{
  htmlSig("<!DOCTYPE HTML"),
  htmlSig("<HTML"),
  htmlSig("<HEAD"),
  htmlSig("<SCRIPT"),
  htmlSig("<IFRAME"),
  htmlSig("<H1"),
  htmlSig("<DIV"),
  htmlSig("<FONT"),
  htmlSig("<TABLE"),
  htmlSig("<A"),
  htmlSig("<STYLE"),
  htmlSig("<TITLE"),
  htmlSig("<B"),
  htmlSig("<BODY"),
  htmlSig("<BR"),
  htmlSig("<P"),
  htmlSig("<!--"),
  &maskedSig{
    mask:   []byte("\xFF\xFF\xFF\xFF\xFF"),
    pat:    []byte("<?xml"),
    skipWS: true,
    ct:     "text/xml; charset=utf-8"},
  &exactSig{[]byte("%PDF-"), "application/pdf"},
  &exactSig{[]byte("%!PS-Adobe-"), "application/postscript"},

  // UTF BOMs.
  &maskedSig{
    mask: []byte("\xFF\xFF\x00\x00"),
    pat:  []byte("\xFE\xFF\x00\x00"),
    ct:   "text/plain; charset=utf-16be",
  },
  &maskedSig{
    mask: []byte("\xFF\xFF\x00\x00"),
    pat:  []byte("\xFF\xFE\x00\x00"),
    ct:   "text/plain; charset=utf-16le",
  },
  &maskedSig{
    mask: []byte("\xFF\xFF\xFF\x00"),
    pat:  []byte("\xEF\xBB\xBF\x00"),
    ct:   "text/plain; charset=utf-8",
  },

  // Image types
  // For posterity, we originally returned "image/vnd.microsoft.icon" from
  // https://tools.ietf.org/html/draft-ietf-websec-mime-sniff-03#section-7
  // https://codereview.appspot.com/4746042
  // but that has since been replaced with "image/x-icon" in Section 6.2
  // of https://mimesniff.spec.whatwg.org/#matching-an-image-type-pattern
  &exactSig{[]byte("\x00\x00\x01\x00"), "image/x-icon"},
  &exactSig{[]byte("\x00\x00\x02\x00"), "image/x-icon"},
  &exactSig{[]byte("BM"), "image/bmp"},
  &exactSig{[]byte("GIF87a"), "image/gif"},
  &exactSig{[]byte("GIF89a"), "image/gif"},
  &maskedSig{
    mask: []byte("\xFF\xFF\xFF\xFF\x00\x00\x00\x00\xFF\xFF\xFF\xFF\xFF\xFF"),
    pat:  []byte("RIFF\x00\x00\x00\x00WEBPVP"),
    ct:   "image/webp",
  },
  &exactSig{[]byte("\x89PNG\x0D\x0A\x1A\x0A"), "image/png"},
  &exactSig{[]byte("\xFF\xD8\xFF"), "image/jpeg"},

  // Audio and Video types
  // Enforce the pattern match ordering as prescribed in
  // https://mimesniff.spec.whatwg.org/#matching-an-audio-or-video-type-pattern
  &maskedSig{
    mask: []byte("\xFF\xFF\xFF\xFF"),
    pat:  []byte(".snd"),
    ct:   "audio/basic",
  },
  &maskedSig{
    mask: []byte("\xFF\xFF\xFF\xFF\x00\x00\x00\x00\xFF\xFF\xFF\xFF"),
    pat:  []byte("FORM\x00\x00\x00\x00AIFF"),
    ct:   "audio/aiff",
  },
  &maskedSig{
    mask: []byte("\xFF\xFF\xFF"),
    pat:  []byte("ID3"),
    ct:   "audio/mpeg",
  },
  &maskedSig{
    mask: []byte("\xFF\xFF\xFF\xFF\xFF"),
    pat:  []byte("OggS\x00"),
    ct:   "application/ogg",
  },
  &maskedSig{
    mask: []byte("\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF"),
    pat:  []byte("MThd\x00\x00\x00\x06"),
    ct:   "audio/midi",
  },
  &maskedSig{
    mask: []byte("\xFF\xFF\xFF\xFF\x00\x00\x00\x00\xFF\xFF\xFF\xFF"),
    pat:  []byte("RIFF\x00\x00\x00\x00AVI "),
    ct:   "video/avi",
  },
  &maskedSig{
    mask: []byte("\xFF\xFF\xFF\xFF\x00\x00\x00\x00\xFF\xFF\xFF\xFF"),
    pat:  []byte("RIFF\x00\x00\x00\x00WAVE"),
    ct:   "audio/wave",
  },
  // 6.2.0.2. video/mp4
  mp4Sig{},
  // 6.2.0.3. video/webm
  &exactSig{[]byte("\x1A\x45\xDF\xA3"), "video/webm"},

  // Font types
  &maskedSig{
    // 34 NULL bytes followed by the string "LP"
    pat: []byte("\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00LP"),
    // 34 NULL bytes followed by \xF\xF
    mask: []byte("\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\xFF\xFF"),
    ct:   "application/vnd.ms-fontobject",
  },
  &exactSig{[]byte("\x00\x01\x00\x00"), "font/ttf"},
  &exactSig{[]byte("OTTO"), "font/otf"},
  &exactSig{[]byte("ttcf"), "font/collection"},
  &exactSig{[]byte("wOFF"), "font/woff"},
  &exactSig{[]byte("wOF2"), "font/woff2"},

  // Archive types
  &exactSig{[]byte("\x1F\x8B\x08"), "application/x-gzip"},
  &exactSig{[]byte("PK\x03\x04"), "application/zip"},
  // RAR's signatures are incorrectly defined by the MIME spec as per
  //    https://github.com/whatwg/mimesniff/issues/63
  // However, RAR Labs correctly defines it at:
  //    https://www.rarlab.com/technote.htm#rarsign
  // so we use the definition from RAR Labs.
  // TODO: do whatever the spec ends up doing.
  &exactSig{[]byte("Rar!\x1A\x07\x00"), "application/x-rar-compressed"},     // RAR v1.5-v4.0
  &exactSig{[]byte("Rar!\x1A\x07\x01\x00"), "application/x-rar-compressed"}, // RAR v5+

  &exactSig{[]byte("\x00\x61\x73\x6D"), "application/wasm"},

  textSig{}, // should be last
}
```

Well. _At least it's standard!_

Except if we [look at the standard](https://mimesniff.spec.whatwg.org/), we learn
what it's for:

> The HTTP `Content-Type` header field is intended to indicate the MIME type of
> an HTTP response. However, many HTTP servers supply a `Content-Type` header
> field value that does not match the actual contents of the response.
> Historically, web browsers have tolerated these servers by examining the
> content of HTTP responses in addition to the `Content-Type` header field in
> order to determine the effective MIME type of the response.
>
> Without a clear specification for how to "sniff" the MIME type, each user
> agent has been forced to reverse-engineer the algorithms of other user agents
> in order to maintain interoperability. Inevitably, these efforts have not
> been entirely successful, resulting in divergent behaviors among user agents.
> In some cases, these divergent behaviors have had security implications, as a
> user agent could interpret an HTTP response as a different MIME type than the
> server intended.
>
> These security issues are most severe when an "honest" server allows
> potentially malicious users to upload their own files and then serves the
> contents of those files with a low-privilege MIME type. For example, if a
> server believes that the client will treat a contributed file as an image
> (and thus treat it as benign), but a user agent believes the content to be
> HTML (and thus privileged to execute any scripts contained therein), an
> attacker might be able to steal the user's authentication credentials and
> mount other cross-site scripting attacks. (Malicious servers, of course, can
> specify an arbitrary MIME type in the `Content-Type` header field.)
>
> This document describes a content sniffing algorithm that carefully balances
> the compatibility needs of user agent with the security constraints imposed
> by existing web content. The algorithm originated from research conducted by
> Adam Barth, Juan Caballero, and Dawn Song, based on content sniffing
> algorithms present in popular user agents, an extensive database of existing
> web content, and metrics collected from implementations deployed to a sizable
> number of users.

A surprisingly readable introduction, for a standard. My understanding is that
this is a standard for user agents to follow, ie., HTTP clients. Why is an HTTP
server implementing this?

Well, because Go is opinionated, of course! This saves us one entire line of
code! Conciseness, yay! [Mr Graham would be so
proud](https://ideolalia.com/essays/thought-leaders-and-chicken-sexers.html).

Unfortunately, this means that, much like everything in Go, simple cases
"usually work", until they don't anymore, and then you better strap in
because you're in for a [wild
ride](/articles/i-want-off-mr-golangs-wild-ride).

What if you need to support a mime type that's not in `sniffSignatures`?
Is that system extensible? Of course not!

`sniffSignatures` is private ("unexported", to be technical), so you can't
add anything to it. It's also a global, so it wouldn't be wise to, anyway.

In that case, you should probably have your _own_ mechanism to tag assets
with their proper `Content-Type`, and set it explicitly, and at this point,
you're paying for the whole "automatic buffering" for no added benefit.

{% sc bearsays %}
It's worth noting that the detection itself is [skipped in that case](https://github.com/golang/go/blob/9c91cab0da9814a598f2c4f7568b6276ff972672/src/net/http/server.go#L1404).
{% endsc %}

That's not the last bit of implicitness going on. The last paragraph for
`http.ResponseWriter.Write` reads:

> Additionally, if the total size of all written data is under a few KB and
> there are no `Flush` calls, the `Content-Length` header is added automatically.

If we read between the lines, that means an `http.ResponseWriter` has an
internal buffer, of "some size that's less than a few kilobytes", which it
uses to sniff the content-type.

Well - actually that's not true. An `http.ResponseWriter` does not have any
internal buffer, because it's an `interface`! Only the implementation given
to you by the `http` package has a buffer. One could totally implement
`http.ResponseWriter` for another type that has completely different
semantics, and then the comments would be completely wrong.

_Unless_ you decide the interface's comments are part of the interface
itself, and then you have, you guessed it - an implicit contract.

Which nothing enforces.

And then we find ourselves in the interesting position where this code is
unsafe:

```go
func doSomething(rw http.ResponseWriter) {
  // 🙅‍♀️ woops, we're casting to a completely different type
  writeStuff(rw)
}

func writeStuff(w io.Writer) {
  w.Write([]byte("stuff"))
}
```

The comments for the `Write` method of `io.Write` do not mention any
content-type sniffing, buffering, or implicit header-writing:

> Writer is the interface that wraps the basic Write method.
>
> Write writes `len(p)` bytes from `p` to the underlying data stream. It returns
> the number of bytes written from `p` (`0 <= n <= len(p)`) and any error
> encountered that caused the write to stop early. Write must return a non-nil
> error if it returns `n < len(p)`. Write must not modify the slice data, even
> temporarily.
>
> Implementations must not retain `p`.

But hey, whatever. It works most of the time. And indeed if we do try
the new version of our origin server, it appears to work fine:

```shell
$ for subdomain in ducks giraffes internal; do domain="${subdomain}.example.org"; echo "\n${domain}"; curl --connect-to "${domain}:80:localhost:8125" "http://${domain}" ; done

ducks.example.org
Have some happy ducks!

giraffes.example.org
Here's a long neck

internal.example.org
[CONFIDENTIAL] The secret ingredient is love
```

## What are we V8ing for? Onwards!

Now onto node.js.

It too, has an `http` package. Heck, it even has an `https` package! And an
`http2` package! Which makes it rather annoying to support all of these! But
not to worry - there's numerous takes on this available today from your local
npm retailer.

Instead of creating a `net.Server`, we now create an `http.Server`:

```javascript
const http = require("http");

async function main() {
  let server = new http.Server({});
  server.on("request", (req, res) => {
    handleRequest(req, res).catch((err) => {
      throw err;
    });
  });
  server.on("error", (err) => {
    throw err;
  });
  let port = 8124;
  server.listen(port, "0.0.0.0", () => {
    console.log(`Now listening on port ${port}`);
  });
}
```

`handleRequest` now uses fields on the objects that the `http` package parsed
for us:

```javascript
async function handleRequest(req, res) {
  console.log(
    `[${req.socket.address().address}] ${JSON.stringify(req.headers, null, 2)}`,
  );

  if (isRestricted(req) && !isAllowed(req.socket.address())) {
    res.statusCode = 403;
    res.end("Forbidden.\n");
    return;
  }

  await proxyRequest(req, res);
}
```

The `isAllowed` and `isRestricted` methods are just as before:

```javascript
function isAllowed(addr) {
  return addr.startsWith("127.0.0.") || addr.startsWith("2.58.12.");
}

function isRestricted(req) {
  return req.headers.Host === "internal.example.org";
}
```

And finally, `proxyRequest` does a bunch of field-copying and piping:

```javascript
async function proxyRequest(req, res) {
  let originReq = new http.ClientRequest(`http://127.00.1:8125${req.url}`);
  // how convenient!
  originReq.headers = req.headers;
  req.pipe(originReq);

  originReq.on("response", (originRes) => {
    res.statusCode = originRes.statusCode;
    res.statusMessage = originRes.statusMessage;
    res.headers = originRes.headers;

    originRes.pipe(res);
  });
}
```

Let's check that our proxy still works. The response from upstream (the Go
service) was:

```shell
$ curl -v http://ducks.example.org --connect-to ducks.example.org:80:localhost:8125
* Connecting to hostname: localhost
* Connecting to port: 8125
*   Trying 127.0.0.1:8125...
* Connected to localhost (127.0.0.1) port 8125 (#0)
> GET / HTTP/1.1
> Host: ducks.example.org
> User-Agent: curl/7.73.0
> Accept: */*
>
* Mark bundle as not supporting multiuse
< HTTP/1.1 200 OK
< Date: Tue, 08 Dec 2020 14:31:47 GMT
< Content-Length: 23
< Content-Type: text/plain; charset=utf-8
<
Have some happy ducks!
* Connection #0 to host localhost left intact
```

And the response from our node.js service is:

```shell
$ curl -v http://ducks.example.org --connect-to ducks.example.org:80:localhost:8124
* Connecting to hostname: localhost
* Connecting to port: 8124
*   Trying 127.0.0.1:8124...
* Connected to localhost (127.0.0.1) port 8124 (#0)
> GET / HTTP/1.1
> Host: ducks.example.org
> User-Agent: curl/7.73.0
> Accept: */*
>
* Mark bundle as not supporting multiuse
< HTTP/1.1 404 Not Found
< Date: Tue, 08 Dec 2020 14:50:48 GMT
< Connection: keep-alive
< Keep-Alive: timeout=5
< Transfer-Encoding: chunked
<
No such domain is hosted on this server
* Connection #0 to host localhost left intact
```

Well... it's getting a 404. But that's not all.

Our upstream service is making use of _all the implicitness we talked about_.
Even though we never specify it, our response has a `Content-Type`, and a
`Content-Length`:

```raw
< Content-Length: 23
< Content-Type: text/plain; charset=utf-8
```

Yet our node.js service replies does _not_ set a `Content-Length`. Instead
it uses [chunked transfer encoding](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Transfer-Encoding).

Let's look at the raw (as raw as curl will let us) answer from node.js:

```shell
$ curl --raw -i http://ducks.example.org --connect-to ducks.example.org:80:localhost:8124
HTTP/1.1 404 Not Found
Date: Tue, 08 Dec 2020 14:54:50 GMT
Connection: keep-alive
Keep-Alive: timeout=5
Transfer-Encoding: chunked

28
No such domain is hosted on this server

0
```

Sure enough, those are chunks. One 28-byte chunk, and a "terminating" 0-byte
chunk. This is peculiar: our upstream response _has_ a `Content-Length`, so
there's no need for chunking.

Maybe in `proxyRequest`?

```javascript
let originReq = new http.ClientRequest(`http://127.00.1:8125${req.url}`);
```

This is correct. `req.url` is a relative URL, it needs to be concatenated to
a "base" URL, which...

{% sc bearsays %}
Hold on a minute... `127.00.1`?
{% endsc %}

{% sc amossays %}
Whoops. I actually did write that. And it actually did work.
{% endsc %}

{% sc bearsays %}
Oh whoa, [RFC 3779](https://tools.ietf.org/html/rfc3779#section-1.1) talks about
that - it's an "abbreviated prefix".
{% endsc %}

{% sc amossays %}
So `127.1` should work?
{% endsc %}

```shell
$ ping 127.1
PING 127.1 (127.0.0.1) 56(84) bytes of data.
64 bytes from 127.0.0.1: icmp_seq=1 ttl=64 time=0.018 ms
64 bytes from 127.0.0.1: icmp_seq=2 ttl=64 time=0.022 ms
```

{% sc bearsays %}
Whoa.
{% endsc %}

How nice! I guess we all learned something today.

Let's fully embrace the typo. So this line, now two bytes shorter:

```javascript
let originReq = new http.ClientRequest(`http://127.1:8125${req.url}`);
```

...seems okay.

What about the next line?

```javascript
originReq.headers = req.headers;
```

{% sc bearsays %}
I don't know, seems okay.
{% endsc %}

Is it? It's true that we do the reverse a couple lines down:

```javascript
originReq.on("response", (originRes) => {
  res.statusCode = originRes.statusCode;
  res.statusMessage = originRes.statusMessage;
  // 👇 here
  res.headers = originRes.headers;

  originRes.pipe(res);
});
```

...and there it seems to work just fine.

But maybe that's where things go wrong? What is the type of
`IncomingMessage.headers` anyway?

{% sc bearsays %}
The.. type? In JS?
{% endsc %}

{% sc amossays %}
Ah, you know what I mean - whatever's on `nodejs.org/docs`.
{% endsc %}

Let's [take a look](https://nodejs.org/docs/latest-v15.x/api/http.html#http_message_headers):

> ### `message.headers`
>
> Added in: v0.1.5
>
> - [\<Object\>](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Object)
>
> The request/response headers object.
>
> Key-value pairs of header names and values. Header names are lower-cased.
>
> ```javascript
> // Prints something like:
> //
> // { 'user-agent': 'curl/7.22.0',
> //   host: '127.0.0.1:8000',
> //   accept: '*/*' }
> console.log(request.headers);
> ```

{% sc amossays %}
Looks like a regular object to me.
{% endsc %}

{% sc bearsays %}
Does it say what happens if you specify some headers multiple times?
{% endsc %}

{% sc amossays %}
As a matter of fact, it does:
{% endsc %}

> Duplicates in raw headers are handled in the following ways, depending on the header name:
>
> - Duplicates of `age`, `authorization`, `content-length`, `content-type`, `etag`, `expires`, `from`, `host`, `if-modified-since`, `if-unmodified-since`, `last-modified`, `location`, `max-forwards`, `proxy-authorization`, `referer`, `retry-after`, `server`, or `user-agent` are discarded.
> - `set-cookie` is always an array. Duplicates are added to the array.
> - For duplicate `cookie` headers, the values are joined together with `'; '`.
> - For all other headers, the values are joined together with `', '`.

{% sc bearsays %}
Whew. That's not a regular object at all.
{% endsc %}

{% sc amossays %}
It does seem to do a fair amount of transformation.
{% endsc %}

Note that this logic _would_ clarify how our first "evil request" should be
handled. Only the first `Host` header counts, the other one is discarded.

{% sc bearsays %}
Just out of curiosity, what's the type of `ClientRequest.headers`?
{% endsc %}

{% sc amossays %}
Well, let's see... ah.
{% endsc %}

{% sc bearsays %}
What?
{% endsc %}

{% sc amossays %}
There's no `ClientRequest.headers` field.
{% endsc %}

{% sc bearsays %}
There's none? Well how come we can assign to it?
{% endsc %}

Well bear, we can assign to it because this is JavaScript, and "fields" on
"objects" are a social construct. The only truth is hashmap (or hash table,
or dictionary, or associative array, or whatever you want to call it).

[TypeScript](https://www.typescriptlang.org/) could save us from that one,
and that's why I swear by it.

So, let's take a look at how we're actually supposed to set headers.

> The header is still mutable using the `setHeader(name, value)`,
> `getHeader(name)`, `removeHeader(name)`

Interesting! So we have to _iterate_ through all the headers from our
incoming request, and set them one by one on the outgoing request.

Something like that:

```javascript
for (const k of Object.keys(req.headers)) {
  originReq.setHeader(k, req.headers[k]);
}
```

With that change, things appear to work:

```shell
$ curl --raw -i http://ducks.example.org --connect-to ducks.example.org:80:localhost:8124
HTTP/1.1 200 OK
Date: Tue, 08 Dec 2020 15:18:06 GMT
Connection: keep-alive
Keep-Alive: timeout=5
Transfer-Encoding: chunked

17
Have some happy ducks!

0
```

Unfortunately, it's still using `transfer-encoding: chunked`.

{% sc bearsays %}
Also, what about multiple headers?
{% endsc %}

{% sc amossays %}
How do you mean?
{% endsc %}

{% sc bearsays %}
Sure, it's not meaningful to have more than one `Host`, unless you're trying
some funny business. But for some _other_ headers, it makes perfect sense.

Try sending multiple `set-cookie` for example?
{% endsc %}

Alrighty, let's make a request with two `Set-Cookie` headers - which is how
you set multiple cookies. It can't be concatenated with `;`, or `,`, because
those both already have meanings in `Set-Cookie` header values.

```shell
$ curl --raw -i http://ducks.example.org --connect-to ducks.example.org:80:localhost:8124 -H "Set-Cookie: one=1" -H "Set-Cookie: two=2"
HTTP/1.1 200 OK
Date: Tue, 08 Dec 2020 15:22:31 GMT
Connection: keep-alive
Keep-Alive: timeout=5
Transfer-Encoding: chunked

17
Have some happy ducks!

0
```

Here's the log output from the node.js service:

```shell
[127.0.0.1] {
  "host": "ducks.example.org",
  "user-agent": "curl/7.73.0",
  "accept": "*/*",
  "set-cookie": [
    "one=1",
    "two=2"
  ]
}
```

{% sc bearsays %}
Innnnteresting. It's an array?
{% endsc %}

It's an array! Remember from the docs:

> `set-cookie` is always an array. Duplicates are added to the array.

{% sc bearsays %}
And that works with `ClientRequest.setHeader`?
{% endsc %}

Well, let's [take a look](https://nodejs.org/docs/latest-v15.x/api/http.html#http_request_setheader_name_value):

> ### `request.setHeader(name, value)`
>
> Added in: v1.6.0
>
> - `name` [\<string\>](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Data_structures#String_type)
> - `value` [\<any\>](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Data_structures#Data_types)
>
> Sets a single header value for headers object. If this header already exists in the to-be-sent headers, its value will be replaced. Use an array of strings here to send multiple headers with the same name. Non-string values will be stored without modification. Therefore, [`request.getHeader()`](https://nodejs.org/docs/latest-v15.x/api/http.html#http_request_getheader_name) may return non-string values. However, the non-string values will be converted to strings for network transmission.
>
> ```javascript
> request.setHeader("Content-Type", "application/json");
> ```
>
> or
>
> ```javascript
> request.setHeader("Cookie", ["type=ninja", "language=javascript"]);
> ```

What a sweet, sweet bag of semantics.

The method sets "a single header value", unless you pass a non-string, in
which case it can be multiple values.

Non-string values are "converted to strings for network transmission", except for
`set-cookie`, which is converted to _multiple header lines_ - if we replace our Go
service with `netcat`, this time in `-l` (listen) mode:

```shell
$ nc -vvv -l localhost -p 8125
Listening on any address 8125
Connection from 127.0.0.1:46986
GET / HTTP/1.1
host: ducks.example.org
user-agent: curl/7.73.0
accept: */*
set-cookie: one=1
set-cookie: two=2
Connection: close
```

And just to finish with the node.js side, turns out this bit of code was
incorrect as well, and was causing the chunking:

```javascript
originReq.on("response", (originRes) => {
  res.statusCode = originRes.statusCode;
  res.statusMessage = originRes.statusMessage;
  // this bit right here:
  res.headers = originRes.headers;

  originRes.pipe(res);
});
```

That's right! An `http.ServerResponse` doesn't have a `headers` field either!

It has: `flushHeaders()`, `getHeader(name)`, `getHeaderNames()`, `getHeaders()`,
`hasHeader(name)`, `removeHeader(name)`, `setHeader(name, value)`, and of course,
`writeHead(statusCode[, statusMessage][, headers])`, _all of which have something
to do with headers_.

What we probably want here is `setHeader(name, value)`. Or do we? Let's
see... if there's multiple values for the same header name, we get an
array... wait, that's only for `set-cookie`. What about the other ones?

Oh they get concatenated, with either `; ` or `, `. Okay. And how does
`setHeader` work? Let's [read the docs](https://nodejs.org/docs/latest-v15.x/api/http.html#http_response_setheader_name_value):

> ### response.setHeader(name, value)
>
> Added in: v0.4.0
>
> - `name` [\<string\>](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Data_structures#String_type)
> - `value` [\<any\>](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Data_structures#Data_types)
>
> Sets a single header value for implicit headers. If this header already exists in the to-be-sent headers, its value will be replaced. Use an array of strings here to send multiple headers with the same name. Non-string values will be stored without modification. Therefore, [`response.getHeader()`](https://nodejs.org/docs/latest-v15.x/api/http.html#http_response_getheader_name) may return non-string values. However, the non-string values will be converted to strings for network transmission.
>
> ```javascript
> response.setHeader("Content-Type", "text/html");
> ```
>
> or
>
> ```javascript
> response.setHeader("Set-Cookie", ["type=ninja", "language=javascript"]);
> ```

{% sc bearsays %}
I'm getting déjà vu.
{% endsc %}

{% sc amossays %}
But wait, there's more!
{% endsc %}

> Attempting to set a header field name or value that contains invalid characters will result in a [`TypeError`](https://nodejs.org/docs/latest-v15.x/api/errors.html#errors_class_typeerror) being thrown.

{% sc bearsays %}
That's defensible.
{% endsc %}

{% sc amossays %}
It goes on!
{% endsc %}

> When headers have been set with [`response.setHeader()`](https://nodejs.org/docs/latest-v15.x/api/http.html#http_response_setheader_name_value), they will be merged with any headers passed to [`response.writeHead()`](https://nodejs.org/docs/latest-v15.x/api/http.html#http_response_writehead_statuscode_statusmessage_headers), with the headers passed to [`response.writeHead()`](https://nodejs.org/docs/latest-v15.x/api/http.html#http_response_writehead_statuscode_statusmessage_headers) given precedence.
>
> ```javascript
> // Returns content-type = text/plain
> const server = http.createServer((req, res) => {
>   res.setHeader("Content-Type", "text/html");
>   res.setHeader("X-Foo", "bar");
>   res.writeHead(200, { "Content-Type": "text/plain" });
>   res.end("ok");
> });
> ```

{% sc bearsays %}
Right, that still makes sense.
{% endsc %}

> If [`response.writeHead()`](https://nodejs.org/docs/latest-v15.x/api/http.html#http_response_writehead_statuscode_statusmessage_headers) method is called and this method has not been called, it will directly write the supplied header values onto the network channel without caching internally, and the [`response.getHeader()`](https://nodejs.org/docs/latest-v15.x/api/http.html#http_response_getheader_name) on the header will not yield the expected result. If progressive population of headers is desired with potential future retrieval and modification, use [`response.setHeader()`](https://nodejs.org/docs/latest-v15.x/api/http.html#http_response_setheader_name_value) instead of [`response.writeHead()`](https://nodejs.org/docs/latest-v15.x/api/http.html#http_response_writehead_statuscode_statusmessage_headers).

{% sc bearsays %}
Seems like a little bit of a gotcha, but also, who would do such a thing?

It's nice that it's mentioned in the docs at least.
{% endsc %}

{% sc amossays %}
It is nice for sure, but you know what would be even nicer?

If one didn't need to read the docs to divine the behavior of those
functions.
{% endsc %}

{% sc bearsays %}
So you're lazy? You're a lazy programmer? You can't be arsed to read docs, is
that it?
{% endsc %}

{% sc amossays %}
That's certainly a popular opinion, yes - and simultaneously, that the articles
I write are too long. But I'm sure there are harder truths to reconcile.
{% endsc %}

{% sc bearsays %}
Don't deflect - what's wrong with reading docs?
{% endsc %}

Well, we've been over this with Go before.

Every time you rely on documentation to enforce correct behavior, you're
exposing the users of your API to potential bugs. Your API is no longer
misuse-resistant.

And you don't need a fancy type checker to do it, either.

If the `ClientRequest` or `ServerResponse` objects were
[sealed](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Object/seal),
and we were using [strict
mode](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Strict_mode),
a `TypeError` could have been thrown when we tried to assign to `res.headers`.

Of course if you _do_ have a fancy type checker, you could catch that error
_before it happens_, which as far as I'm concerned, is the absolute dream.

{% sc bearsays %}
So that's a "yes" on the lazy thing?
{% endsc %}

Not quite - I wouldn't say I'm "lazy", but I am a "realist".

{% sc bearsays %}
Oh boy, there he goes.
{% endsc %}

_Ideally_,
everyone reads all the docs all the time, including whenever upgrading
dependencies, and nobody ever breaks [semantic
versioning](https://semver.org/), and we're all smart enough to write C code
that isn't [a trash fire waiting to
happen](/articles/working-with-strings-in-rust).

But _in actuality_, semver breakage happens all the dang time, we're all
exhausted and occasionally tell dependabot to just "rebase and push" at the
end of a long workday, and the [CVE database](https://cve.mitre.org/) is not
going out of business any time soon.

From what we've seen so far, here are some of the things we could do in
node.js, that would look _totally normal and innocent_ in code review, but
are actually way broken:

1. Assign to `request.headers` or `response.headers` - those fields don't
   exist
2. Call `request.setHeader("set-cookie", "a=b")`, then call
   `request.setHeader("set-cookie", "c=d")` (the second value would overwrite
   the first one)
3. Treat `message.headers["some-key"]` like a string (not true for `set-cookie`)
4. Try to forward all headers by using `response.setHeader` on all the key-value
   pairs from a request.

{% sc bearsays %}
Wait, how is that last one wrong?
{% endsc %}

I'm so glad you asked! You see, node.js _does_ quite a bit of transformation before
populating, say, `message.headers`.

So if there _were_ multiples of a header it didn't know about, it would join them
together with `,`. But what if that's not what you wanted?

If you're writing a proxy, you may want to forward the headers more or less
untouched, minus maybe some headers that are protected/sensitive, and maybe
adding one or two headers which are internal.

{% sc bearsays %}
Can't you do that in node.js?
{% endsc %}

You **totally can** do that in node.js, thanks to [rawHeaders](https://nodejs.org/docs/latest-v15.x/api/http.html#http_message_rawheaders):

> ### `message.rawHeaders`
>
> Added in: v0.11.6
>
> - [\<string[]\>](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Data_structures#String_type)
>
> The raw request/response headers list exactly as they were received.
>
> The keys and values are in the same list. It is *not* a list of tuples. So, the even-numbered offsets are key values, and the odd-numbered offsets are the associated values.
>
> Header names are not lowercased, and duplicates are not merged.
>
> ```javascript
> // Prints something like:
> //
> // [ 'user-agent',
> //   'this is invalid because there can be only one',
> //   'User-Agent',
> //   'curl/7.22.0',
> //   'Host',
> //   '127.0.0.1:8000',
> //   'ACCEPT',
> //   '*/*' ]
> console.log(request.rawHeaders);
> ```

So technically, all we need to do is this:

```javascript
for (let i = 0; i < req.rawHeaders.length; i += 2) {
  let k = req.rawHeaders[i];
  let v = req.rawHeaders[i + 1];
  originReq.setHeader(k, v);
}
```

And everything should work out

Let's use `netcat` as a listener again:

```shell
$ curl --raw -i http://ducks.example.org --connect-to ducks.example.org:80:localhost:8124 -H "Set-Cookie: one=1" -H "Set-Cookie: two=2"
```

```shell
$ nc -vvv -l localhost -p 8125
Listening on any address 8125
Connection from 127.0.0.1:47416
GET / HTTP/1.1
Host: ducks.example.org
User-Agent: curl/7.73.0
Accept: */*
Set-Cookie: two=2
Connection: close
```

Wait, where did `one=1` go?

{% sc bearsays %}
Uhhh if you call `setHeader` with the same name twice, it overwrites...
{% endsc %}

Oh right! Haha. That's number 2 on the list. We were warned, and we stepped
right in it anyway.

So what's the correct way do to it? Well... there's no method of
`ClientRequest` that lets us pass "raw headers", unlike `ServerResponse`,
which has `writeHead`.

Sure, we could do something like this:

```javascript
// First, collect all raw headers into a Map<String, Array<String>>
let headers = {};
for (let i = 0; i < req.rawHeaders.length; i += 2) {
  let k = req.rawHeaders[i];
  let v = req.rawHeaders[i + 1];
  headers[k] = [...(headers[k] || []), v];
}
for (const k of Object.keys(headers)) {
  let vv = headers[k];
  // `vv` is a non-string, so node.js should "leave them alone"
  // and only "transform them to strings" when sending them over
  // the network.
  originReq.setHeader(k, vv);
}
```

This _would_ let our two `Set-Cookie` lines pass:

```shell
$ nc -vvv -l localhost -p 8125
Listening on any address 8125
Connection from 127.0.0.1:47522
GET / HTTP/1.1
Host: ducks.example.org
User-Agent: curl/7.73.0
Accept: */*
Set-Cookie: one=1
Set-Cookie: two=2
Connection: close
```

Unless one of them had a slightly different casing...

```shell
$ curl --raw -i http://ducks.example.org --connect-to ducks.example.org:80:localhost:8124 -H "Set-Cookie: one=1" -H "set-Cookie: two=2"
```

(The second is `set-Cookie`, with a lowercase `s`)

...and then only one of them would pass:

```shell
$ nc -vvv -l localhost -p 8125
Listening on any address 8125
Connection from 127.0.0.1:47538
GET / HTTP/1.1
Host: ducks.example.org
User-Agent: curl/7.73.0
Accept: */*
set-Cookie: two=2
Connection: close
```

We could of course normalize the casing ourselves to all-lowercase - or
something else - but then we're back to transforming headers and we're not
being a very transparent proxy.

As far as I'm concerned, I don't see a way to make a node.js `ClientRequest`
send multiple headers, some of which only differ from the others by their
casing.

{% sc bearsays %}
Amos, that's silly.
{% endsc %}

{% sc amossays %}
Amos, that's silly who?
{% endsc %}

{% sc bearsays %}
Amos, that's silly: no application would actually depend on header casing.

It's right there in [RFC 2616, section 4.2](https://www.ietf.org/rfc/rfc2616.html#section-4.2):

> Field names are case-insensitive.
> {% endsc %}

{% sc amossays %}
You'd be surprised.
{% endsc %}

Speaking of being surprised... we made pretty significant changes when we
ported our node.js access control service to the node.js `http` module.

Does it even still work?

```shell
$ curl -i http://internal.example.org --connect-to internal.example.org:80:172.30.84.116:8124
HTTP/1.1 200 OK
Date: Tue, 08 Dec 2020 16:36:45 GMT
Connection: keep-alive
Keep-Alive: timeout=5
Transfer-Encoding: chunked

[CONFIDENTIAL] The secret ingredient is love
```

Oh.

Oh no.

It does not work at all.

Let's look at the access control code:

```javascript
function isRestricted(req) {
  return req.headers.Host === "internal.example.org";
}
```

It's been so long since we wrote this code, I had completely forgotten about it.

{% sc bearsays %}
That's a LIE! You've planned EVERYTHING!
{% endsc %}

...just like in the real world. Code is written, shipped, and forgotten. It
is only remembered when it misbehaves, which is pretty sad if you think about
it.

So let's not think about it.

As it turns out, node.js normalizes header names to lower cases. We've read
that before, in the middle of _all the docs we read_ (who's lazy now?), it was
spelled out:

> Header names are lower-cased.

So we can use our knowledge of the implementation and just access the `host`
field, all lowercase:

```javascript
function isRestricted(req) {
  return req.headers.host === "internal.example.org";
}
```

And then, everything works fin-

```shell
$ node index.js
Now listening on port 8124
[172.30.84.116] {
  "host": "internal.example.org",
  "user-agent": "curl/7.73.0",
  "accept": "*/*"
}
/home/amos/ftl/correctness/http/acl-js/index.js:63
  return addr.startsWith("127.0.0.") || addr.startsWith("2.58.12.");
              ^

TypeError: addr.startsWith is not a function
    at isAllowed (/home/amos/ftl/correctness/http/acl-js/index.js:63:15)
    at handleRequest (/home/amos/ftl/correctness/http/acl-js/index.js:24:29)
    at Server.<anonymous> (/home/amos/ftl/correctness/http/acl-js/index.js:6:5)
    at Server.emit (node:events:376:20)
    at parserOnIncoming (node:_http_server:919:12)
    at HTTPParser.parserOnHeadersComplete (node:_http_common:126:17)
```

Oh no. Our `isAllowed` function is wrong too! Or maybe we're just calling it
wrong! Who knows? We don't have a fancy type checker! We read docs 😎

So the documentation for `isAllowed` is... we didn't write any.

But [the documentation]() for `request.socket.address()` is:

> ### `socket.address()`
>
> Added in: v0.1.90
>
> - Returns: [\<Object\>](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Object)
>
> Returns the bound `address`, the address `family` name and `port` of the socket as reported by the operating system: `{ port: 12346, family: 'IPv4', address: '127.0.0.1' }`

Which returns... an `Object`, so far so good, with fields `port`, `family`,
and `address`. Ah, there it is! What we want is
`request.socket.address().address`.

It somehow got lost in the port (no pun intended). And this time I _swear_ I
didn't do a mistake on purpose, just to illustrate this article.

{% sc bearsays %}
Yeah right.
{% endsc %}

While we're fixing this bug, let's do a pass over the whole code. We'll give
up on proxying the headers as-is. Apparently that's just not something the
node.js `http` package is meant for - which is fine! Not everything needs to
be general-purpose.

So let's just use `setHeader` for the `ClientRequest`, and let's use `writeHead`
for the `ServerResponse`, which is the closest we can reasonably get today.

{% sc tip %}
Note that `writeHead` accepts _either_ raw headers or normalized headers,
which means it must be able to distinguish between an "object" (or hash map,
or hash table, or dictionary, or associative array) and an "array".

I wonder what it does when you pass an array with an odd number of fields. So
much delicious undefined behavior! But there's only so much that's fit to print.
You try it and [report back](https://twitter.com/fasterthanlime)!
{% endsc %}

So, without further ado, here's the final version of our node.js code:

```javascript
const http = require("http");

// an IIFE (immediately-invoked function expression), just for fun
(function () {
  let server = new http.Server({});
  server.on("request", handleRequest);
  server.on("error", (err) => {
    throw err;
  });
  let port = 8124;
  server.listen(port, "0.0.0.0", () => {
    console.log(`Now listening on port ${port}`);
  });
})();

// none of what we were doing was async, so it's all
// old-style node.js callbacks now
function handleRequest(req, res) {
  console.log(
    `[${req.socket.address().address}] ${JSON.stringify(req.headers, null, 2)}`,
  );

  if (isRestricted(req) && !isAllowed(req.socket.address().address)) {
    res.statusCode = 403;
    res.end("Forbidden.\n");
    return;
  }

  let originReq = new http.ClientRequest(`http://127.1:8125${req.url}`);
  // forward client request headers to origin
  for (const k of Object.keys(req.headers)) {
    originReq.setHeader(k, req.headers[k]);
  }
  // forward client request body to origin
  req.pipe(originReq);

  originReq.on("response", (originRes) => {
    // forward origin response headers to client
    res.writeHead(
      originRes.statusCode,
      originRes.statusMessage,
      originRes.rawHeaders,
    );
    // forward origin response body to client
    originRes.pipe(res);
  });
}

function isAllowed(addr) {
  return addr.startsWith("127.0.0.") || addr.startsWith("2.58.12.");
}

function isRestricted(req) {
  return req.headers.host === "internal.example.org";
}
```

And just like that, our access control service is, again, controlling access:

```shell
$ curl -i http://internal.example.org --connect-to internal.example.org:80:172.30.84.116:8124
HTTP/1.1 403 Forbidden
Date: Tue, 08 Dec 2020 17:03:27 GMT
Connection: keep-alive
Keep-Alive: timeout=5
Content-Length: 11

Forbidden.
```

```shell
$ curl -i http://internal.example.org --connect-to internal.example.org:80:localhost:8124
HTTP/1.1 200 OK
Date: Tue, 08 Dec 2020 17:03:33 GMT
Content-Length: 45
Content-Type: text/plain; charset=utf-8
Connection: close

[CONFIDENTIAL] The secret ingredient is love
```

And as a bonus - it's not chunking anymore! Because we're setting the
`content-length` we get from origin on the `ServerResponse`, node.js knows
that chunking is not necessary because we know the length of the full
response.

{% sc recap %}
There are many, _many_ ways to misuse the node.js APIs. Even when reading
docs, those mistakes do happen. Some of them result in runtime errors, and
some of them just silently do the wrong thing.
{% endsc %}

And this is where we stop looking at node.js.

Well... no. We should take our final code and let TypeScript check it.

## A bit of TypeScript, as a treat

I don't feel like setting up the whole compilation pipeline, but we can get
TypeScript to _only_ do type checking of our `.js` file. If we just slap
`//@ts-check` at the top of our file, [VS Code](https://code.visualstudio.com/) has us covered.

First off, it's unhappy about our `handleRequest` function:

> Parameter `req` implicitly has an `any` type, but a better type may be inferred from usage.

Actually inferring it from usage results in a pretty lengthy type, based on,
well, usage - so after it does that, there's no longer any errors, but it also
doesn't match the node.js API, just "how we use it":

```javascript
/**
 * @param {{ socket: { address: () => { (): any; new (): any; address: any; }; }; headers: { [x: string]: string | number | readonly string[]; }; url: any; pipe: (arg0: import("http").ClientRequest) => void; }} req
 * @param {{ statusCode: number; end: (arg0: string) => void; writeHead: (arg0: number, arg1: string, arg2: string[]) => void; }} res
 */
function handleRequest(req, res) {
  // etc.
}
```

Instead, what we want is this:

```javascript
/**
 * @param {http.IncomingMessage} req
 * @param {http.ServerResponse} res
 */
function handleRequest(req, res) {
  // etc.
}
```

And if we do this, it finds two errors!

```shell
$ tsc --noEmit --allowJs ./index.js
index.js:23:30 - error TS2339: Property 'address' does not exist on type '{} | AddressInfo'.
  Property 'address' does not exist on type '{}'.

23     `[${req.socket.address().address}] ${JSON.stringify(req.headers, null, 2)}`
                                ~~~~~~~

index.js:26:60 - error TS2339: Property 'address' does not exist on type '{} | AddressInfo'.
  Property 'address' does not exist on type '{}'.

26   if (isRestricted(req) && !isAllowed(req.socket.address().address)) {
                                                              ~~~~~~~


Found 2 errors.
```

Well. That's rather unhelpful. I _can_ see a `net.Socket` returning an empty
object (although, why not `null`?) if we call `address()` before it's connected,
like so:

```shell
$ node -i
Welcome to Node.js v15.3.0.
Type ".help" for more information.
> let sock = new require("net").Socket();
undefined
> sock.address()
{}
>
```

...but in this case, that can never happen: `handleRequest` is only ever
passed to `server.on("request", ...)`, and so it only ever gets instances of
`http.IncomingMessage`, whose `socket`s are _always_ connected, so
`address()` never returns `{}`.

So, that's a false positive: the type checker is reporting an error where
there is none. I can see that it's just trying to be cautious - things may be
fine now, but what if we called `handleRequest` for somewhere else, with a
carefully-crafted `http.IncomingMessage` whose `socket` was _not_ connected?

Then who would be the wiser? tsc, no doubt.

But in the meantime, let's use the escape hatch TypeScript gives us and just
add a `!` after accessing the field:

```javascript
  console.log(
    `[${req.socket.address().address!}] ${JSON.stringify(req.headers, null, 2)}`
  );

  if (isRestricted(req) && !isAllowed(req.socket.address().address!)) {
    res.statusCode = 403;
    res.end("Forbidden.\n");
    return;
  }
```

```shell
$ tsc --noEmit --allowJs ./index.js
index.js:23:9 - error TS8013: Non-null assertions can only be used in TypeScript files.

23     `[${req.socket.address().address!}] ${JSON.stringify(req.headers, null, 2)}`
           ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

index.js:26:39 - error TS8013: Non-null assertions can only be used in TypeScript files.

26   if (isRestricted(req) && !isAllowed(req.socket.address().address!)) {
                                         ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~


Found 2 errors.
```

Wait, nope, we're still writing vanilla JavaScript, just with [JSDoc
annotations](https://jsdoc.app/). I guess we'll just have to uh... be creative:

```javascript
// safety: the `net.Socket` from `http.IncomingMessage` are always connected,
// so the address is never `{}`
/** @type {import("net").AddressInfo} */
// @ts-ignore
let address = req.socket.address().address;

console.log(`[${address}] ${JSON.stringify(req.headers, null, 2)}`);

if (isRestricted(req) && !isAllowed(address)) {
  res.statusCode = 403;
  res.end("Forbidden.\n");
  return;
}
```

There! Now we no longer have errors.

Thankfully, TypeScript has a secret reserve of errors called "strict mode", which
enables a bunch more checks, and since we want our code to be really high-quality,
we might as opt into it:

```shell
$ tsc --noEmit --allowJs --strict ./index.js
index.js:39:28 - error TS2345: Argument of type 'string | string[] | undefined' is not assignable to parameter of type 'string | number | readonly string[]'.
  Type 'undefined' is not assignable to type 'string | number | readonly string[]'.

39     originReq.setHeader(k, req.headers[k]);
                              ~~~~~~~~~~~~~~

index.js:47:7 - error TS2345: Argument of type 'number | undefined' is not assignable to parameter of type 'number'.
  Type 'undefined' is not assignable to type 'number'.

47       originRes.statusCode,
         ~~~~~~~~~~~~~~~~~~~~

index.js:56:20 - error TS7006: Parameter 'addr' implicitly has an 'any' type.

56 function isAllowed(addr) {
                      ~~~~

index.js:60:23 - error TS7006: Parameter 'req' implicitly has an 'any' type.

60 function isRestricted(req) {
                         ~~~


Found 4 errors.
```

Let's tackle the bottom two, since they're easy. We know `isAllowed` takes a `string`, and
`isRestricted` takes a `http.IncomingMessage`.

```javascript
/**
 * @param {string} addr
 */
function isAllowed(addr) {
  return addr.startsWith("127.0.0.") || addr.startsWith("2.58.12.");
}

/**
 * @param {http.IncomingMessage} req
 */
function isRestricted(req) {
  return req.headers.host === "internal.example.org";
}
```

Ahhh. Better.

{% sc tip %}
Note that this wouldn't have caught our little `req.headers.Host` mishap.

As neat as it is, TypeScript does not let you define a type that "only
has lower-cased keys"...
{% endsc %}

Correct! in fact, here's the type of `IncomingHttpHeaders`:

```typescript
// incoming headers will never contain number
interface IncomingHttpHeaders extends NodeJS.Dict<string | string[]> {
  accept?: string;
  "accept-language"?: string;
  "accept-patch"?: string;
  "accept-ranges"?: string;
  "access-control-allow-credentials"?: string;
  "access-control-allow-headers"?: string;
  "access-control-allow-methods"?: string;
  "access-control-allow-origin"?: string;
  "access-control-expose-headers"?: string;
  "access-control-max-age"?: string;
  "access-control-request-headers"?: string;
  "access-control-request-method"?: string;
  age?: string;
  allow?: string;
  "alt-svc"?: string;
  authorization?: string;
  "cache-control"?: string;
  connection?: string;
  "content-disposition"?: string;
  "content-encoding"?: string;
  "content-language"?: string;
  "content-length"?: string;
  "content-location"?: string;
  "content-range"?: string;
  "content-type"?: string;
  cookie?: string;
  date?: string;
  expect?: string;
  expires?: string;
  forwarded?: string;
  from?: string;
  host?: string;
  "if-match"?: string;
  "if-modified-since"?: string;
  "if-none-match"?: string;
  "if-unmodified-since"?: string;
  "last-modified"?: string;
  location?: string;
  origin?: string;
  pragma?: string;
  "proxy-authenticate"?: string;
  "proxy-authorization"?: string;
  "public-key-pins"?: string;
  range?: string;
  referer?: string;
  "retry-after"?: string;
  "sec-websocket-accept"?: string;
  "sec-websocket-extensions"?: string;
  "sec-websocket-key"?: string;
  "sec-websocket-protocol"?: string;
  "sec-websocket-version"?: string;
  "set-cookie"?: string[];
  "strict-transport-security"?: string;
  tk?: string;
  trailer?: string;
  "transfer-encoding"?: string;
  upgrade?: string;
  "user-agent"?: string;
  vary?: string;
  via?: string;
  warning?: string;
  "www-authenticate"?: string;
}
```

Which is straight-up _hilarious_.

Mostly, it's done that way so that:

- `set-cookie` is _always_ a `string[]`
- Other known headers are always just a `string`

But it also extends `NodeJS.Dict<string | string[]>`, which means that _any
other header_ can be either a `string` or a `string[]` (or `undefined`).

At any rate, the following code isn't an error at all:

```javascript
/**
 * @param {http.IncomingMessage} req
 */
function isRestricted(req) {
  return req.headers.Host === "internal.example.org";
}
```

But this code is:

```javascript
/**
 * @param {http.IncomingMessage} req
 */
function isRestricted(req) {
  return req.headers["set-cookie"] === "internal.example.org";
}
```

```shell
index.js:67:10 - error TS2367: This condition will always return 'false' since the types 'string[] | undefined' and 'string' have no overlap.

67   return req.headers["set-cookie"] === "internal.example.org";
            ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
```

Good for `set-cookie`.

Let's look at our remaining errors:

```shell
index.js:30:39 - error TS2345: Argument of type 'AddressInfo' is not assignable to parameter of type 'string'.

30   if (isRestricted(req) && !isAllowed(address)) {
                                         ~~~~~~~
```

Woops, this one is legit! We accidentally declared `let address` as an
`AddressInfo`, when it's really a `string`, because we're accessing
`socket.address().address`, remember?

Let's fix it:

```javascript
// safety: the `net.Socket` from `http.IncomingMessage` are always connected,
// so the address is never `{}`
/** @type {string} */
// @ts-ignore
let address = req.socket.address().address;
```

Next up:

```shell
index.js:47:7 - error TS2345: Argument of type 'number | undefined' is not assignable to parameter of type 'number'.
  Type 'undefined' is not assignable to type 'number'.

47       originRes.statusCode,
         ~~~~~~~~~~~~~~~~~~~~
```

That one's a bit annoying. If the server does not respond with a status code,
well... wouldn't parsing fail way before then? We wouldn't even get headers!

But sure, let's be "correct":

```javascript
originReq.on("response", (originRes) => {
  if (!originRes.statusCode) {
    res.writeHead(502, "Oh hey y'all are back early");
    res.end("Origin's haunted.");
    return;
  }

  // forward origin response headers to client
  res.writeHead(
    originRes.statusCode,
    originRes.statusMessage,
    originRes.rawHeaders,
  );
  // forward origin response body to client
  originRes.pipe(res);
});
```

Amazingly, this is enough for `tsc` to figure out that if we reach the second
`res.writeHead`, then `originRes.statusCode` _cannot be falsy_, so this took
care of that error.

(This is not sarcastic btw, I genuinely like TypeScript a lot. It's the best
of a very messy situation).

Finally, we're left with this error:

```shell
$ index.js:39:28 - error TS2345: Argument of type 'string | string[] | undefined' is not assignable to parameter of type 'string | number | readonly string[]'.
  Type 'undefined' is not assignable to type 'string | number | readonly string[]'.

39     originReq.setHeader(k, req.headers[k]);
                              ~~~~~~~~~~~~~~
```

That one's annoying - but illuminating. Much easier than reading the docs.

{% sc bearsays %}
Okay I'm halfway onboard the lazy train now - this _is_ nicer than opening
the docs. You can even keep your browser closed.

I could get used to this.
{% endsc %}

So, according to the types we're seeing here, `requests[k]` _could_ be `undefined`.

I'm not sure I agree, but maybe it's confused by our usage of `Object.keys`?

```javascript
for (const k of Object.keys(req.headers)) {
  originReq.setHeader(k, req.headers[k]);
}
```

If I hover the `k` in `const k`, it just says `string`, which - okay, yeah, if we
look up _arbitrary_ header names, we might get `undefined`. Otherwise, we won't.

We can fix it like this:

```javascript
// forward client request headers to origin
for (const k of Object.keys(req.headers)) {
  // this is solely to make the type checker happy
  let v = req.headers[k];
  if (v) {
    originReq.setHeader(k, v);
  }
}
```

Which is... not great, because we're adding an `if` branch solely for
type-checking purposes, and I don't think it'll be eliminated. It might be
deemed "unlikely" by the JIT and the "it's never undefined" path may become
the fast path, but that's for me to ignore and you to profile.

The other option is to use `//@ts-ignore`, but it's a bit too much of a
shotgun blast for my taste, since it disables checking for the _whole line_.
What if that line was doing something else wrong? Uncaught errors! The
horror!

{% sc recap %}
TypeScript can help catch _some_ misuses of the node.js APIs, but not all of them!

Sometimes, it _thinks_ it's caught errors, but really, it's only just getting
in the way.

This is not really TypeScript's fault. Typings for a package can only be as
good as the original package. If a function returns `string | number |
readonly string[]`, well, all bets are off.
{% endsc %}

## Just gopher it

It is time... to look at Go again. If anything, we've learn that accurately
modelling HTTP, even just HTTP headers, is harder than it appears at first
glance.

Do you remember, ages ago, when someone confidently said that?

> HTTP/1.1 seems like a pretty simple protocol...

How foolish it seems now! Utter hogwash.

Sometimes things are just complicated!

And it's not like you can convince everyone to speak a particular flavor of
HTTP; those services are meant to be user-facing, handling requests from a
variety of user agents, some of which are malicious, while the rest are
merely misguided (which is programmer for "opinionated, but in a way that's
not to my advantage").

So, let's take a look at how Go tackles this problem. But be warned: I'm
going to say nice things about it.

{% sc bearsays %}
Whaaaaaaaat? But that goes against the preconceived notion that so many people
have of you, you can't just-
{% endsc %}

{% sc amossays %}
Hate to interrupt you bear, but I think we've blown past our quota for "meta
banter" several pages ago, we better get on with it.
{% endsc %}

So, we've looked at some of the types that node.js uses to represent headers,
and so far we've had:

- An object, whose keys are always lower-case (unless you mess with it... but
  plz refrain), and whose values are always strings, unless they're arrays
  of strings, which `set-cookie` always is, but others might be too,
  according to the TypeScript typings.

And then:

- An array of length `n*2`, where even positions contain header names (that's right,
  `0` is even), and odd positions contain header values.

[The documentation](https://nodejs.org/docs/latest-v15.x/api/http.html#http_message_rawheaders) takes special care to note that:

> The keys and values are in the same list. It is **not** a list of tuples.
> So, the even-numbered offsets are key values, and the odd-numbered offsets
> are the associated values.

This sounds a little funky at first, until you realize that, well, JavaScript
does not have tuples, so it would have to be an array of arrays, and that
ends up being a _lot_ of allocations, and even more importantly, a lot of
[GC](<https://en.wikipedia.org/wiki/Garbage_collection_(computer_science)>)
bookkeeping.

So, enough with the suspense - what does Go do?

Well first off, `Go` actually takes the `host` header and extracts it to a
separate field:

```go
type Request struct {
  // (other fields are omitted)

  // For server requests, Host specifies the host on which the
  // URL is sought. For HTTP/1 (per RFC 7230, section 5.4), this
  // is either the value of the "Host" header or the host name
  // given in the URL itself. For HTTP/2, it is the value of the
  // ":authority" pseudo-header field.
  // It may be of the form "host:port". For international domain
  // names, Host may be in Punycode or Unicode form. Use
  // golang.org/x/net/idna to convert it to either format if
  // needed.
  // To prevent DNS rebinding attacks, server Handlers should
  // validate that the Host header has a value for which the
  // Handler considers itself authoritative. The included
  // ServeMux supports patterns registered to particular host
  // names and thus protects its registered Handlers.
  //
  // For client requests, Host optionally overrides the Host
  // header to send. If empty, the Request.Write method uses
  // the value of URL.Host. Host may contain an international
  // domain name.
  Host string
}
```

What the comment omits is that, for HTTP/1, only the _first_ `Host` header is
taken into account - which sounds reasonable, and matches what node.js does.

What the comment does point out, is that this struct also works for
HTTP/2 - it simply jams the `:authority` pseudo-header in there (as per
[RFC 7540](https://tools.ietf.org/html/rfc7540#section-8.1.2.3)).

{% sc bearsays %}
...do you think linking RFCs will make commenters easier on you? Because
that's not going to work.
{% endsc %}

{% sc amossays %}
Look,
{% endsc %}

Similarly, `Content-Length` has its own field:

```go
  // ContentLength records the length of the associated content.
  // The value -1 indicates that the length is unknown.
  // Values >= 0 indicate that the given number of bytes may
  // be read from Body.
  //
  // For client requests, a value of 0 with a non-nil Body is
  // also treated as unknown.
  ContentLength int64
```

There's also fields for `TransferEncoding`, and `Connection: Close`.

As for the other headers, well, there's `Header`:

```go
  // Header contains the request header fields either received
  // by the server or to be sent by the client.
  //
  // If a server received a request with header lines,
  //
  //  Host: example.com
  //  accept-encoding: gzip, deflate
  //  Accept-Language: en-us
  //  fOO: Bar
  //  foo: two
  //
  // then
  //
  //  Header = map[string][]string{
  //    "Accept-Encoding": {"gzip, deflate"},
  //    "Accept-Language": {"en-us"},
  //    "Foo": {"Bar", "two"},
  //  }
  //
  // For incoming requests, the Host header is promoted to the
  // Request.Host field and removed from the Header map.
  //
  // HTTP defines that header names are case-insensitive. The
  // request parser implements this by using CanonicalHeaderKey,
  // making the first character and any characters following a
  // hyphen uppercase and the rest lowercase.
  //
  // For client requests, certain headers such as Content-Length
  // and Connection are automatically written when needed and
  // values in Header may be ignored. See the documentation
  // for the Request.Write method.
  Header Header
```

There's a lot to unpack here, so let's go paragraph by paragraph:

```go
  // Header contains the request header fields either received
  // by the server or to be sent by the client.
```

Go uses the same types for sending and receiving requests, which is occasionally
convenient, and often a very large [footgun](https://en.wiktionary.org/wiki/footgun)
since some fields may only make sense when sending, and others while receiving.

```go
  // If a server received a request with header lines,
  //
  //  Host: example.com
  //  accept-encoding: gzip, deflate
  //  Accept-Language: en-us
  //  fOO: Bar
  //  foo: two
  //
  // then
  //
  //  Header = map[string][]string{
  //    "Accept-Encoding": {"gzip, deflate"},
  //    "Accept-Language": {"en-us"},
  //    "Foo": {"Bar", "two"},
  //  }
```

Here we can see the _actual_ underlying type of `Header`: a `map[string][]string`. Or,
as we've spelled it before, in TypeScript parlance, a `Map<String, Array<String>>`.

{% sc bearsays %}
...which was not quite accurate, as an [ES6
Map](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Map)
is not the same as an Object.
{% endsc %}

Basically, it means that for every header name, we have an array (well, a Go slice) of
potential values.

Which leaves us with the problem of the header _names_, which should be case-insensitive.

As we can see from the example, and unlike node.js, Go's `http` package does
_not_ just lower-case everything. It takes the surprising approach of...
making everything Title-Case:

```go
  // HTTP defines that header names are case-insensitive. The
  // request parser implements this by using CanonicalHeaderKey,
  // making the first character and any characters following a
  // hyphen uppercase and the rest lowercase.
```

Here's the actual implementation of `CanonicalHeaderKey`:

```go
// CanonicalHeaderKey returns the canonical format of the
// header key s. The canonicalization converts the first
// letter and any letter following a hyphen to upper case;
// the rest are converted to lowercase. For example, the
// canonical key for "accept-encoding" is "Accept-Encoding".
// If s contains a space or invalid header field bytes, it is
// returned without modifications.
func CanonicalHeaderKey(s string) string { return textproto.CanonicalMIMEHeaderKey(s) }
```

...fine, here's the actual implementation of `CanonicalMimeHeaderKey`:

```go
// CanonicalMIMEHeaderKey returns the canonical format of the
// MIME header key s. The canonicalization converts the first
// letter and any letter following a hyphen to upper case;
// the rest are converted to lowercase. For example, the
// canonical key for "accept-encoding" is "Accept-Encoding".
// MIME header keys are assumed to be ASCII only.
// If s contains a space or invalid header field bytes, it is
// returned without modifications.
func CanonicalMIMEHeaderKey(s string) string {
  commonHeaderOnce.Do(initCommonHeader)

  // Quick check for canonical encoding.
  upper := true
  for i := 0; i < len(s); i++ {
    c := s[i]
    if !validHeaderFieldByte(c) {
      return s
    }
    if upper && 'a' <= c && c <= 'z' {
      return canonicalMIMEHeaderKey([]byte(s))
    }
    if !upper && 'A' <= c && c <= 'Z' {
      return canonicalMIMEHeaderKey([]byte(s))
    }
    upper = c == '-'
  }
  return s
}
```

{% sc bearsays %}
Hey! That's not utf-8 safe!
{% endsc %}

{% sc amossays %}
Doesn't matter. As per RFC 2616, header names are "tokens", and a "token"
is "at least 1 of: any CHAR except CTLs or separators", and a "CHAR" is
"any US-ASCII character (octets 0 - 127)".
{% endsc %}

This is actually only the checking code - the fast path. That's right, **Go
servers are slower if you send them lower-case headers**. Maybe we shouldn't
have written our access control service in node.js!

Here's where the actual mutation happens:

```go
// canonicalMIMEHeaderKey is like CanonicalMIMEHeaderKey but is
// allowed to mutate the provided byte slice before returning the
// string.
//
// For invalid inputs (if a contains spaces or non-token bytes), a
// is unchanged and a string copy is returned.
func canonicalMIMEHeaderKey(a []byte) string {
  // See if a looks like a header key. If not, return it unchanged.
  for _, c := range a {
    if validHeaderFieldByte(c) {
      continue
    }
    // Don't canonicalize.
    return string(a)
  }

  upper := true
  for i, c := range a {
    // Canonicalize: first letter upper case
    // and upper case after each dash.
    // (Host, User-Agent, If-Modified-Since).
    // MIME headers are ASCII only, so no Unicode issues.
    if upper && 'a' <= c && c <= 'z' {
      c -= toLower
    } else if !upper && 'A' <= c && c <= 'Z' {
      c += toLower
    }
    a[i] = c
    upper = c == '-' // for next time
  }
  // The compiler recognizes m[string(byteSlice)] as a special
  // case, so a copy of a's bytes into a new string does not
  // happen in this map lookup:
  if v := commonHeader[string(a)]; v != "" {
    return v
  }
  return string(a)
}
```

This code actually mentions that "MIME headers are ASCII only". That's genuinely nice.

This part is not so nice:

```go
  // The compiler recognizes m[string(byteSlice)] as a special
  // case, so a copy of a's bytes into a new string does not
  // happen in this map lookup:
  if v := commonHeader[string(a)]; v != "" {
```

But I guess [strings are hard](/articles/working-with-strings-in-rust).

So, in Go:

- Any header name can have any number of header values
- All header names are normalized to `Weird-Pre-Http2-Case`

And indeed, the `Header` type (which really should be called `HeaderMap`, or
`Headers` or something, but conciseness above all!!) comes with a collection
of handy methods.

It has `Add(key, value string)`, it has `Clone()`, `Del(key string)`,
`Get(key string)`, `Set(key, value string)`, and `Values(key string)
[]string`.

It also has `Write(w io.Writer) error` and `WriteSubset(w io.Writer, exclude
map[string]bool) error` - the latter feels like a strange addition, but maybe
there's a good reason for it.

Here's the thing though - `Header` is not a `struct`.

It's just a type definition. (Not a type alias - those are different!).

Here it is:

```go
type Header map[string][]string
```

Which means that any function that can operate on a `map[k]v`, can also
operate on a `Header`.

So... you could totally still have the same bug we had in node.js:

```go
package main

import (
  "log"
  "net/http"
)

func main() {
  // This is constructed properly, according to the "contract" written
  // in `http.Header`'s documentation:
  headers := http.Header{
    "Host": []string{"internal.example.org"},
  }

  // but it is parsed incorrectly:
  log.Printf("Is this endpoint restricted? %v", isRestricted(headers))
}

func isRestricted(headers http.Header) bool {
  // nothing is preventing us from doing this
  for _, v := range headers["host"] {
    if v == "internal.example.org" {
      return true
    }
  }
  return false
}
```

This prints:

```shell
2009/11/10 23:00:00 Is this endpoint restricted? false
```

Similarly, there's nothing that prevents us from constructing an instance of `http.Header`
that contradicts its documentation:

```go
package main

import (
  "net/http"
  "os"
)

func main() {
  headers := http.Header{
    "Host": []string{"internal.example.org"},
    "host": []string{"ducks.example.org"},
  }

  headers.Write(os.Stdout)
}
```

Also, and this one is completely gratuitous, you can construct this, which makes no sense whatsoever:

```go
package main

import (
  "net/http"
  "os"
)

func main() {
  headers := http.Header{
    "secure": []string{},
  }

  headers.Write(os.Stdout)
}
```

This just doesn't write anything.

But if someone were to check whether the key's in the map...

```go
package main

import (
  "log"
  "net/http"
  "os"
)

func main() {
  headers := http.Header{
    "secure": []string{},
  }

  if _, ok := headers["secure"]; ok {
    log.Println("This request is secure!")
  }

  headers.Write(os.Stdout)
}
```

```shell
2009/11/10 23:00:00 This request is secure!
```

And this all stems from one of the aspects of Go I've discussed before, which
is that the shortcuts that were taken when designing its type system makes it
a language that's both very confusing (in lieu of being "simple") _and_ that
consistently resists modelling reality.

A good example of this is zero values.

Go fields _cannot_ be uninitialized, because every type has a zero value.

```go
package main

import "log"

// A Foobarist can foobar. This comment brought to you by `go-lint`.
type Foobarist interface {
  Foobar()
}

func main() {
  var x int
  var s string
  var fb Foobarist
  var sl []string
  log.Printf("x = %#v", x)
  log.Printf("s = %#v", s)
  log.Printf("fb = %#v", fb)
  log.Printf("sl = %#v", sl)
}
```

```shell
$ go run main.go
2020/12/11 21:50:32 x = 0
2020/12/11 21:50:32 s = ""
2020/12/11 21:50:32 fb = <nil>
2020/12/11 21:50:32 sl = []string(nil)
```

{% sc bearsays %}
And yes, there's [a well-known
gotcha](https://yourbasic.org/golang/gotcha-why-nil-error-not-equal-nil/)
around `nil` and interfaces, but that's not what we're discussing here.
{% endsc %}

So, if we make a `struct`, it too will have a zero value.

```go
package main

import "log"

// A Profile is a Profile. This comment brought to you by big tautology.
type Profile struct {
  Name string
  Bio  string
}

func main() {
  var pf Profile
  log.Printf("pf = %#v", pf)
}
```

Now imagine that `Profile` is being persisted to a database somewhere.

Let's make a quick in-memory database for demonstration purposes. We'll need
a `go.mod`:

```raw
module go-musings

go 1.15
```

```go
// in `go-musings/database/database.go`

package database

// A Profile is a Profile.
type Profile struct {
  Name string
  Bio  string
}

type DB struct {
  seed    int64
  records map[int64]Profile
}

func NewDB() DB {
  return DB{
    seed:    0,
    records: make(map[int64]Profile),
  }
}

func (db *DB) Insert(profile Profile) int64 {
  id := db.seed
  db.seed++
  db.records[id] = profile
  return id
}

func (db *DB) Get(id int64) Profile {
  return db.records[id]
}

func (db *DB) Update(id int64, profile Profile) {
  db.records[id] = profile
}
```

Such an API lets us do many things! We can insert a profile into the
database, then get it, update one field, and get it again:

```go
// in `go-musings/main.go`

package main

import (
  "log"

  "go-musings/database"
)

func main() {
  db := database.NewDB()

  pf := database.Profile{
    Name: "Lilibet",
    Bio:  "I don't want even *want* to be queen and yet my sister is jealous.",
  }
  id := db.Insert(pf)

  // Update the name
  {
    pf := db.Get(id)
    pf.Name = "Elizabeth"
    db.Update(id, pf)
  }

  log.Printf("%#v", db.Get(id))
}
```

But what if we wanted to update a record _without_ retrieving it?

Something like this:

```go
func main() {
  db := database.NewDB()

  pf := database.Profile{
    Name: "Lilibet",
    Bio:  "I don't want even *want* to be queen and yet my sister is jealous.",
  }
  id := db.Insert(pf)

  // Update the name - without getting first!
  db.Update(id, database.Profile{Name: "Elizabeth"})

  log.Printf("%#v", db.Get(id))
}
```

In terms of performance, this can make a big difference. We no longer have to
read and deserialize _all the fields_ from the database only to put them back
again. Writes can be batched transparently so they can be executed very
rapidly, instead of constantly blocking because we're waiting for reads to be
done.

But with our current design, it does not work, because it resets
`Profile.Bio` to its zero value:

```shell
$ go run main.go
2020/12/11 22:18:41 database.Profile{Name:"Elizabeth", Bio:""}
```

So, when I say "Go fields cannot be uninitialized", it doesn't mean "the
compiler will make sure you initialize everything to some meaningful value".
It means "if you don't, the compiler will insert zero values, which may or
may not make sense for your application".

Of course, not all hope is lost - we can adjust our database implementation
to _only_ update fields that have non-zero values set:

```go
// in `go-musings/database/database.go`

func (db *DB) Update(id int64, profile Profile) {
  // pretend this isn't just a dumb `map[k]v` and
  // we can actually update things in-place, otherwise
  // none of this makes any sense.
  pf := db.records[id]
  changed := false
  if profile.Name != "" {
    pf.Name = profile.Name
    changed = true
  }
  if profile.Bio != "" {
    pf.Bio = profile.Bio
    changed = true
  }
  if changed {
    db.records[id] = pf
  }
}
```

Now our program actually works! We can update the `Name` while leaving the `Bio` alone.

```shell
$ go run main.go
2020/12/11 22:23:43 database.Profile{Name:"Elizabeth", Bio:"I don't want even *want* to be queen and yet my sister is jealous."}
```

...but now there's something we can no longer do. If we try to _just_ clear the `Bio`:

```go
func main() {
  db := database.NewDB()

  pf := database.Profile{
    Name: "Lilibet",
    Bio:  "I don't want even *want* to be queen and yet my sister is jealous.",
  }
  id := db.Insert(pf)

  // Remove the bio
  db.Update(id, database.Profile{Bio: ""})

  log.Printf("%#v", db.Get(id))
}
```

...then nothing happens:

```shell
$ go run main.go
2020/12/11 22:26:48 database.Profile{Name:"Lilibet", Bio:"I don't want even *want* to be queen and yet my sister is jealous."}
```

Because, thanks to zero values, there's no difference between any of these:

```go
db.Update(id, database.Profile{})
db.Update(id, database.Profile{Name: ""})
db.Update(id, database.Profile{Bio: ""})
db.Update(id, database.Profile{Name: "", Bio: ""})
```

So, let's think of a way to address this. We could replace our `string` with
`*string` - pointers to a string.

```go
// A Profile is a Profile.
type Profile struct {
  Name *string
  Bio  *string
}
```

And then... well then we have a bunch of things to worry about.

First off, when we insert a `Profile`, we want to default to `""` for all
fields, because that's _still_ our zero value:

```go
func (db *DB) Insert(profile Profile) int64 {
  id := db.seed
  db.seed++
  if profile.Name == nil {
    var s = ""
    profile.Name = &s
  }
  if profile.Bio == nil {
    var s = ""
    profile.Bio = &s
  }
  db.records[id] = profile
  return id
}
```

And then in `Update`, we only update if non-nil:

```go
func (db *DB) Update(id int64, profile Profile) {
  // again, pretend this isn't a `map[k]v` and we can update things in-place
  pf := db.records[id]
  changed := false
  if profile.Name != nil {
    pf.Name = profile.Name
    changed = true
  }
  if profile.Bio != nil {
    pf.Bio = profile.Bio
    changed = true
  }
  if changed {
    db.records[id] = pf
  }
}
```

So, where do we stand now?

Well, everything is terribly unergonomic:

```go
// in `go-musings/main.go`

package main

import (
  "log"

  "go-musings/database"
)

func main() {
  db := database.NewDB()

  name := "Lilibet"
  bio := "I don't want even *want* to be queen and yet my sister is jealous."
  pf := database.Profile{
    Name: &name,
    Bio:  &bio,
  }
  id := db.Insert(pf)

  // Remove the bio
  newBio := ""
  db.Update(id, database.Profile{Bio: &newBio})

  log.Printf("%#v", db.Get(id))
}
```

But we can make a little `stringptr` function to help a little:

```go
// in `go-musings/main.go`

package main

import (
  "log"

  "go-musings/database"
)

func stringptr(s string) *string {
  return &s
}

func main() {
  db := database.NewDB()

  pf := database.Profile{
    Name: stringptr("Lilibet"),
    Bio:  stringptr("I don't want even *want* to be queen and yet my sister is jealous."),
  }
  id := db.Insert(pf)

  // Remove the bio
  db.Update(id, database.Profile{Bio: stringptr("")})

  log.Printf("%#v", db.Get(id))
}
```

And it _does_ work:

```go
$ go run main.go
2020/12/11 22:36:37 database.Profile{Name:(*string)(0xc0000961e0), Bio:(*string)(0xc000096200)}
```

Well.. it's hard to tell that it works, because the default debug formatter
will _not_ show you what a `*string` points to, but if we use something
slightly friendlier, like [spew](https://pkg.go.dev/github.com/davecgh/go-spew/spew):

```go
package main

import (
  "go-musings/database"

  "github.com/davecgh/go-spew/spew"
)

func stringptr(s string) *string {
  return &s
}

func main() {
  db := database.NewDB()

  pf := database.Profile{
    Name: stringptr("Lilibet"),
    Bio:  stringptr("I don't want even *want* to be queen and yet my sister is jealous."),
  }
  id := db.Insert(pf)

  // Remove the bio
  db.Update(id, database.Profile{Bio: stringptr("")})

  spew.Dump(db.Get(id))
}
```

```shell
$ go run main.go
(database.Profile) {
 Name: (*string)(0xc0001102b0)((len=7) "Lilibet"),
 Bio: (*string)(0xc0001102d0)("")
}
```

There! We did it!

{% sc bearsays %}
🎉!
{% endsc %}

Of course, all of that is only an option if you have the luxury of defining
the struct yourself.

Which you don't, if, for example, you use a code generator like the
[protobuf](https://developers.google.com/protocol-buffers) compiler for Go,
which always generates `string` fields, even though in proto3 all fields are
optional.

So, in that scenario, you have absolutely no way to tell between an "unset
field" and "the empty string". Which, sure, doesn't matter _most of the time_.

Until it does, and well... what do you do then?

Well, you signal whether a field is set or not out-of-band, of course!

With something like:

```go
type Profile struct {
  Name string
  HasName bool
  Bio string
  HasBio bool
}
```

Sounds ridiculous? Well, that's exactly how Go maps work.

If you have a `map[string]string`, and you try to get an entry that does not
exist you get... the zero value for a string, ie. `""`:

```go
package main

import "log"

func main() {
  m := make(map[string]string)
  m["i-do-exist"] = ""
  log.Printf("%#v", m["i-do-exist"])
  log.Printf("%#v", m["i-do-not-exist"])
}
```

```shell
$ go run main.go
2020/12/11 23:03:27 ""
2020/12/11 23:03:27 ""
```

How do you know if it's _actually_ in the map? Well, indexing a map actually
returns two values, so if you assign both of them, you can get that info - as
I mentioned, out of band:

```go
package main

import "log"

func main() {
  m := make(map[string]string)
  m["i-do-exist"] = ""

  {
    v, ok := m["i-do-exist"]
    log.Printf("%#v, %#v", v, ok)
  }
  {
    v, ok := m["i-do-not-exist"]
    log.Printf("%#v, %#v", v, ok)
  }
}
```

```shell
$ go run main.go
2020/12/11 23:05:03 "", true
2020/12/11 23:05:03 "", false
```

If you have a `string` and a `bool`, you have four possible combinations:

1. the string is empty and the bool is `false`
2. the string is empty and the bool is `true`
3. the string is non-empty and the bool is `false`
4. the string is non-empty and the bool is `true`

Combination 3 is _never_ returned when indexing a map in Go, but it's...
there. It's expressible. If we were able to implement our _own_ data
structures that supported indexing, and the standard interface was something
like:

```go
// as of Go 1.15, generics are not a thing (*also* not the topic of this post)
// anyway, use your imagination:
type K string
type V string
type Index interface {
  Get(k K) (V, bool)
}
```

...then nothing at all would prevent us from returning `"lol", false`.

Even without combination 3 being constructed, multi-return and out-of-band
"setness" signalling are the source of _so many_ application bugs.

Of course, it never segfaults. So it's better than C, right? Because memory
safety, yay! It just silently does the wrong thing. So now vulnerabilities are
caused by logic errors instead of corrupted memory.

{% sc bearsays %}
This _does_ sound better, though.
{% endsc %}

{% sc amossays %}
Yeah. Then again, that's a pretty low bar. Bash is memory-safe too!
{% endsc %}

{% sc bearsays %}
Right... so is Excel. Hopefully?
{% endsc %}

{% sc amossays %}
Hopefully.
{% endsc %}

One of the big selling point of Go is "we removed the footguns!" but... did
you? Seems like we just traded weapons. We're _very much_ still in "just be
careful" territory.

## Just don't write bugs!

And this leads me to one of the central points of this... _looks at time
estimate_ this essay I guess.

I made three claims about Rust earlier:

1. Programming in Rust requires you to think differently
2. It is harder to write any code at all in Rust
3. It is easier to write "correct" code in Rust

The first two claims are easy to accept for anyone trying out Rust for the
first time. The third one is another affair entirely.

See, if all you have are the first two claims, it's pretty easy to conclude
that Rustaceans are either masochists (which... who's asking?) _or_ that they
just like things that are hard because they're hard and that makes them feel
clever.

But here's the thing: **Rust is not specifically designed for clever people**.

Quite the contrary in fact. Look at me! Trying to make those subtle points
online! What a stupid, stupid idea. Only grief can come out of this. Clearly
"clever" is not a good descriptor here.

The corollary of claim 3 is: **it is harder to write "correct" code in other
languages**. And by other languages, I'm again thinking in particular of
JavaScript, Ruby, Lua, Go, C, C#, Java, etc. - not Haskell.

Here's one thing that's often said and _sounds_ superior, but isn't:
**Learning Rust made me a better programmer**.

Mostly because, after many rounds of, uh, friendly negotiation with the
compiler, it's made me so much more aware of the sheer amount of things that
can go wrong in a program.

And it's not like Rust _made_ me paranoid. I was aware of _most_ of these
failure conditions before picking up Rust. But the Rust compiler forces you
to address these upfront.

The whole language encourages you to model your program in such a way that
you don't leave anything to chance. That things that should not happen are
either not modelled at all, handled explicitly, or halt the program safely.

In Rust, if you have a "string" field that must be set, you just say this:

```rust
struct Person {
  name: String,
}
```

It _has_ to be initialized. It doesn't just default to the empty string.

This is a compile error:

```rust
fn main() {
    let p = Person {};
}
```

```shell
$ cargo check
    Checking rust-musings v0.1.0 (/home/amos/ftl/correctness/rust-musings)
error[E0063]: missing field `name` in initializer of `Person`
 --> src/main.rs:6:13
  |
6 |     let p = Person {};
  |             ^^^^^^ missing `name`
```

If you _want_ it to default to the empty string, you can implement the
`Default` trait for your struct, and _explicitly_ say that it should
use the default values for any unspecified fields:

```rust
#[derive(Default, Debug)]
struct Person {
    name: String,
}

fn main() {
    let p = Person {
        ..Default::default()
    };
    dbg!(p);
}
```

```shell
$ cargo run -q
[src/main.rs:10] p = Person {
    name: "",
}
```

And if that field is optional... well you explicitly make it optional:

```rust
#[derive(Default, Debug)]
struct Person {
    name: Option<String>,
}

fn main() {
    let p = Person {
        name: Some("Elizabeth".into()),
    };
    dbg!(p);
    let p = Person {
        ..Default::default()
    };
    dbg!(p);
}

```

In which case the field is either `Some("some string")`, or `None`:

```shell
$ cargo run -q
[src/main.rs:10] p = Person {
    name: Some(
        "Elizabeth",
    ),
}
[src/main.rs:14] p = Person {
    name: None,
}
```

And that's also the way a `HashMap` works. When indexing a `HashMap`, you
either get a `Some(value)`, or a `None`. It only returns "one thing".

```rust
use std::collections::HashMap;

fn main() {
    let mut map: HashMap<String, String> = Default::default();
    map.insert("i-exist".into(), "yay".into());

    dbg!(map.get("i-exist"));
    dbg!(map.get("i-do-not-exist"));
}
```

```shell
$ cargo run -q
[src/main.rs:7] map.get("i-exist") = Some(
    "yay",
)
[src/main.rs:8] map.get("i-do-not-exist") = None
```

And you can't accidentally pretend you got a value when you really didn't -
you need to handle both cases, one way or the other:

```rust
use std::collections::HashMap;

fn main() {
    let mut map: HashMap<String, String> = Default::default();
    map.insert("foo".into(), "bar".into());

    // stops program with a generic error message if value isn't `Some`
    print_str(map.get("foo").unwrap());

    // stops program with a custom error message if value isn't `Some`
    print_str(map.get("foo").expect("we wanted foo to be set"));

    // only executed if return value is `Some`
    if let Some(s) = map.get("foo") {
        print_str(s);
    }

    // handles both cases explicitly
    match map.get("foo") {
        Some(s) => {
            print_str(s);
        }
        None => {
            // do nothing
        }
    }
}

fn print_str(s: &str) {
    dbg!(s);
}
```

```shell
$ cargo run -q
[src/main.rs:27] s = "bar"
[src/main.rs:27] s = "bar"
[src/main.rs:27] s = "bar"
[src/main.rs:27] s = "bar"
```

All this isn't at the expense of performance, either. An `Option<&T>` is the
same size as a `*const T` - it's just `None` if the pointer is null.

{% sc tip %}
You normally wouldn't experience raw pointers unless you're writing `unsafe`
code on purpose, when doing [FFI](https://doc.rust-lang.org/nomicon/ffi.html) for example.
{% endsc %}

This is just one of the _many_ ways Rust lets you model what actually happens
in your program. And once you're past the initial frustration, and you really
see the value proposition, everything else feels terribly uncomfortable.

Writing JavaScript and Go is _terrifying_ to me now. All the pitfalls I
already knew about before picking up Rust still exist, but now it's all the
more obvious that _there's no systemic way to avoid them_.

You "just have to be careful".

Which of course never actually works.

Proponents of the "just be careful" mantra (C advocates in particular) will
tell you that anyone who wrote a bug just isn't an experienced enough
programmer - as if we were all engaged in some permanent game of battle
royale.

This is, to put it mildly, self-aggrandizing horseshit.

Engineering is not about "not doing mistakes". Engineering is about **designing
systems that ensure fewer mistakes occur**.

Rust is such a system.

## I think we were talking about HTTP?

Right! HTTP.

Let's take another look at some of the data structures used to represent HTTP
requests and responses in Go.

We've already discussed `Request.Header`, which is a `map[string][]string` in
disguise. But it doesn't end there.

For incoming requests, the protocol version is stored in no less than three fields!

```go
  // The protocol version for incoming server requests.
  //
  // For client requests, these fields are ignored. The HTTP
  // client code always uses either HTTP/1.1 or HTTP/2.
  // See the docs on Transport for details.
  Proto      string // "HTTP/1.0"
  ProtoMajor int    // 1
  ProtoMinor int    // 0
```

Again, that means we can construct nonsensical inputs, like:

```go
  req := Request {
    Proto: "HTTP/1.1",
    ProtoMajor: 2,
    ProtoMinor: 0,
  }
```

One slightly more correct way to do it would be to have a separate struct
type, `HTTPVersion`, that only stores the Major and Minor version:

```go
  type HTTPVersion struct {
    Major int
    Minor int
  }
```

...and have it implement `String()`, so you can have a String representation
whenever needed:

```go
  func (hv HTTPVersion) String() string {
    return fmt.Sprintf("HTTP/%v.%v", hv.Major, hv.Minor)
  }
```

Although that would still leave several issues: you could still build
non-existent (definitely non-supported) versions of HTTP, like `4.-7`.

You could also _still_ mutate `Major` and `Minor`, since they're public
(exported) fields, so in Go, you'd have no choice but to unexport them and
add getters - and then you'd need a constructor, too:

```go
  type HTTPVersion struct {
    major int
    minor int
  }

  func NewHTTPVersion(major int, minor int) HTTPVersion {
    return HTTPVersion { major, minor }
  }

  func (hv HTTPVersion) Major() int {
    return hv.major
  }

  func (hv HTTPVersion) Minor() int {
    return hv.minor
  }
```

Let's look at other fields, like... `ContentLength`:

```go
  // ContentLength records the length of the associated content.
  // The value -1 indicates that the length is unknown.
  // Values >= 0 indicate that the given number of bytes may
  // be read from Body.
  //
  // For client requests, a value of 0 with a non-nil Body is
  // also treated as unknown.
  ContentLength int64
```

Mhhh, using `-1` to signal that the length is unknown. Sounds familiar?

We're using in-band signalling now! Reserving some values to indicate
specific conditions. What does a value of -2 through -9223372036854775808
mean?

It goes on:

```go
  // URL specifies either the URI being requested (for server
  // requests) or the URL to access (for client requests).
  //
  // For server requests, the URL is parsed from the URI
  // supplied on the Request-Line as stored in RequestURI.  For
  // most requests, fields other than Path and RawQuery will be
  // empty. (See RFC 7230, Section 5.3)
  //
  // For client requests, the URL's Host specifies the server to
  // connect to, while the Request's Host field optionally
  // specifies the Host header value to send in the HTTP
  // request.
  URL *url.URL
```

More dual-purpose fields! For client requests, `URL` is the full, absolute
URL you want to request, and so the Host is set.

But for server requests, `URL` is just a relative URL, and it's the `Host`
field that counts.

Why? I don't know! You tell me! All the pieces were there!

Speaking of `URL`, here's its definition:

```go
type URL struct {
  Scheme      string
  Opaque      string    // encoded opaque data
  User        *Userinfo // username and password information
  Host        string    // host or host:port
  Path        string    // path (relative paths may omit leading slash)
  RawPath     string    // encoded path hint (see EscapedPath method)
  ForceQuery  bool      // append a query ('?') even if RawQuery is empty
  RawQuery    string    // encoded query values, without '?'
  Fragment    string    // fragment for references, without '#'
  RawFragment string    // encoded fragment hint (see EscapedFragment method)
}
```

At a glance, just looking at this definition, try to guess - how should you
build a fragment?

As as reminder, a "fragment" is the part of the URL that is not sent to the
server, it's only accessible to the user agent:

```raw
https://example.org?query#fragment
                         ^^^^^^^^^
```

So, when building a `URL` to be formatted, should we set `Fragment` or `RawFragment`?

Well, we can look at [the documentation for
`URL.String()`](https://pkg.go.dev/net/url#URL.String). As usual with Go APIs
that "look simple", it's not:

> `String` reassembles the URL into a valid URL string.
>
> The general form of the result is one of:
>
> ```
> scheme:opaque?query#fragment
> scheme://userinfo@host/path?query#fragment
> ```
>
> If `u.Opaque` is non-empty, `String` uses the first form; otherwise it uses the
> second form. Any non-ASCII characters in host are escaped. To obtain the
> path, `String` uses `u.EscapedPath()`.
>
> In the second form, the following rules apply:
>
> - if `u.Scheme` is empty, scheme: is omitted.
> - if `u.User` is nil, userinfo@ is omitted.
> - if `u.Host` is empty, host/ is omitted.
> - if `u.Scheme` and `u.Host` are empty and `u.User` is nil,
>   the entire scheme://userinfo@host/ is omitted.
> - if `u.Host` is non-empty and `u.Path` begins with a /,
>   the form host/path does not add its own /.
> - if `u.RawQuery` is empty, ?query is omitted.
> - if `u.Fragment` is empty, #fragment is omitted.

The answer was `u.Fragment`, because `URL` escapes it, via... `EscapedFragment()`, which
has this documentation:

> `EscapedFragment` returns the escaped form of `u.Fragment`.
>
> In general there are multiple possible escaped forms of any fragment.
>
> `EscapedFragment` returns `u.RawFragment` when it is a valid escaping of `u.Fragment`.
>
> Otherwise `EscapedFragment` ignores `u.RawFragment` and computes an escaped
> form on its own.
>
> The `String` method uses `EscapedFragment` to construct its result. In general, code
> should call `EscapedFragment` instead of reading `u.RawFragment` directly.

So, to get the full picture, we had to look at the definition of the `URL`
struct, its `String()` method, and, to further understand what `String()`
does, its `EscapedFragment()` method. That's assuming the documentation is
up-to-date.

Maintaining _both_ the escaped and non-escaped fragment might make sense from
a performance standpoint - if you parse an incoming request and forward it
somewhere else, there's no need to re-escape the fragment, you can just
forward the "raw fragment" you got in the first place.

But by storing both as exported fields and letting the user manipulate
either, the designers of this bit of the Go API have drawn themselves into a
corner, where they had to add complicated semantics to all functions that
touch either variant of the fragment so that it "makes sense most of the
time".

I'm going to stop showing you Go APIs now because I've used up my sigh
reserve, but if you're brave enough to keep looking at them, you'll see
those patterns used _all over_.

Reading those and thinking, _really thinking_ about the implications of their
design is going to be more convincing than any amount of material I can
personally write, so, by all means, go and do it.

But before you do - let's look at how some of these problems are modelled
by popular Rust crates for HTTP.

## A look at hyper

[hyper](https://crates.io/crates/hyper) is one of my favorite crates. But I
could say that about a lot of crates.

It's a low-level HTTP library, consisting of quality building blocks.

Let's look at the definition of a `Request` in hyper:

```rust
pub struct Request<T> {
    head: Parts,
    body: T,
}
```

Okay, so a `Request` is generic over its body type. Why? Because the body can
be anything. It can be a string in memory, or it can be a bunch of bytes
(a `Vec<u8>` or equivalent), also in memory, or it can be a File, from which
you can read, or it can be another thing that can be streamed.

The only requirement for a body is that you can poll it for data and trailers
(because yes, trailing HTTP headers are a thing which we _will not_ discuss).

Then there's the head, a `Parts`:

```rust
/// Component parts of an HTTP `Request`
///
/// The HTTP request head consists of a method, uri, version, and a set of
/// header fields.
pub struct Parts {
    /// The request's method
    pub method: Method,

    /// The request's URI
    pub uri: Uri,

    /// The request's version
    pub version: Version,

    /// The request's headers
    pub headers: HeaderMap<HeaderValue>,

    /// The request's extensions
    pub extensions: Extensions,

    _priv: (),
}
```

Interesting! There's no `host` field here. Only a `uri`.

The `method` field is an opaque type:

```rust
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Method(Inner);
```

...that wraps a private enum, which accommodates well-known HTTP methods
_and_ extensions:

```rust
#[derive(Clone, PartialEq, Eq, Hash)]
enum Inner {
    Options,
    Get,
    Post,
    Put,
    Delete,
    Head,
    Trace,
    Connect,
    Patch,
    // If the extension is short enough, store it inline
    ExtensionInline([u8; MAX_INLINE], u8),
    // Otherwise, allocate it
    ExtensionAllocated(Box<[u8]>),
}
```

{% sc tip %}
A trick similar to the `smartstring` crate is used here.

You can read more about it in [Peeking inside a Rust enum](/articles/peeking-inside-a-rust-enum).
{% endsc %}

The `version` field is an opaque type:

```rust
pub struct Version(Http);

impl Version {
    /// `HTTP/0.9`
    pub const HTTP_09: Version = Version(Http::Http09);

    /// `HTTP/1.0`
    pub const HTTP_10: Version = Version(Http::Http10);

    /// `HTTP/1.1`
    pub const HTTP_11: Version = Version(Http::Http11);

    /// `HTTP/2.0`
    pub const HTTP_2: Version = Version(Http::H2);

    /// `HTTP/3.0`
    pub const HTTP_3: Version = Version(Http::H3);
}
```

..which wraps a private enum, containing all the supported versions of HTTP:

```rust
#[derive(PartialEq, PartialOrd, Copy, Clone, Eq, Ord, Hash)]
enum Http {
    Http09,
    Http10,
    Http11,
    H2,
    H3,
    __NonExhaustive,
}
```

...and provides a `Debug` implementation to format it as a string:

```rust
impl fmt::Debug for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::Http::*;

        f.write_str(match self.0 {
            Http09 => "HTTP/0.9",
            Http10 => "HTTP/1.0",
            Http11 => "HTTP/1.1",
            H2 => "HTTP/2.0",
            H3 => "HTTP/3.0",
            __NonExhaustive => unreachable!(),
        })
    }
}
```

Note that there is absolutely no way (in safe code) to construct an HTTP
version that's meaningless.

But what's particularly interesting is how HTTP headers are represented.

The `headers` field is of type `HeaderMap` which is defined as follows:

```rust
pub struct HeaderMap<T = HeaderValue> {
    // Used to mask values to get an index
    mask: Size,
    indices: Box<[Pos]>,
    entries: Vec<Bucket<T>>,
    extra_values: Vec<ExtraValue<T>>,
    danger: Danger,
}
```

Well.. it's not a `Vec<(String, String)>`. And it's not a `HashMap<String, String>`.

It's not a `HashMap<String, Vec<String>>` either.

It's a _multimap_ (like a hashmap, but each key can have multiple values), of
`HeaderName` to `HeaderValue`.

All through `hyper`, we're following the principle of "you can only build
something that's meaningful".

So for example, in Go you can do this:

```go
package main

import (
  "fmt"
  "net/http"
  "strings"
)

func main() {
  headers := make(http.Header)
  headers.Add("Host", "example.org")
  headers.Add("Née", "élégante")

  sb := new(strings.Builder)
  headers.Write(sb)
  fmt.Printf("%s\n", sb)
}
```

And generate non-compliant HTTP headers:

```shell
$ go run main.go
Host: example.org
Née: élégante
```

But when using hyper in Rust, you can't build it.

The following is a compile-time error:

```rust
use hyper::HeaderMap;

fn main() {
    let mut headers = HeaderMap::new();
    headers.insert("Née", "élégante");
}
```

```shell
$ cargo check --quiet
error[E0308]: mismatched types
 --> src/main.rs:5:27
  |
5 |     headers.insert("Née", "élégante");
  |                           ^^^^^^^^^^ expected struct `HeaderValue`, found `&str`

error: aborting due to previous error
```

It wants a `HeaderValue`. And you can only build a `HeaderValue` if you
pass.. a valid header value, which this is not, so this is a runtime error:

```rust
use hyper::{header::HeaderValue, HeaderMap};

fn main() {
    let mut headers = HeaderMap::new();
    headers.insert("Née", HeaderValue::from_static("élégante"));
}
```

```shell
$ RUST_BACKTRACE=1 cargo run --quiet
thread 'main' panicked at 'invalid header value', /home/amos/.cargo/registry/src/github.com-1ecc6299db9ec823/http-0.2.1/src/header/value.rs:64:17
stack backtrace:
   0: std::panicking::begin_panic
             at /home/amos/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/std/src/panicking.rs:505
   1: http::header::value::HeaderValue::from_static
             at /home/amos/.cargo/registry/src/github.com-1ecc6299db9ec823/http-0.2.1/src/header/value.rs:64
   2: rust_musings::main
             at ./src/main.rs:5
   3: core::ops::function::FnOnce::call_once
             at /home/amos/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/core/src/ops/function.rs:227
note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.
```

Similarly, if we "fix" our header value, but keep an invalid header name,
we'll also panic (ie. the program will safely stop):

```rust
use hyper::{header::HeaderValue, HeaderMap};

fn main() {
    let mut headers = HeaderMap::new();
    headers.insert("Née", HeaderValue::from_static("elegant"));
}
```

```shell
$ RUST_BACKTRACE=1 cargo run --quiet
thread 'main' panicked at 'static str is invalid name: InvalidHeaderName', /home/amos/.cargo/registry/src/github.com-1ecc6299db9ec823/http-0.2.1/src/header/name.rs:2042:64
stack backtrace:
   0: rust_begin_unwind
             at /rustc/7eac88abb2e57e752f3302f02be5f3ce3d7adfb4/library/std/src/panicking.rs:483
   1: core::panicking::panic_fmt
             at /rustc/7eac88abb2e57e752f3302f02be5f3ce3d7adfb4/library/core/src/panicking.rs:85
   2: core::option::expect_none_failed
             at /rustc/7eac88abb2e57e752f3302f02be5f3ce3d7adfb4/library/core/src/option.rs:1234
   3: core::result::Result<T,E>::expect
             at /home/amos/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/core/src/result.rs:933
   4: http::header::name::HdrName::from_static
             at /home/amos/.cargo/registry/src/github.com-1ecc6299db9ec823/http-0.2.1/src/header/name.rs:2042
   5: <&str as http::header::map::into_header_name::Sealed>::insert
             at /home/amos/.cargo/registry/src/github.com-1ecc6299db9ec823/http-0.2.1/src/header/map.rs:3312
   6: http::header::map::HeaderMap<T>::insert
             at /home/amos/.cargo/registry/src/github.com-1ecc6299db9ec823/http-0.2.1/src/header/map.rs:1137
   7: rust_musings::main
             at ./src/main.rs:5
   8: core::ops::function::FnOnce::call_once
             at /home/amos/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/core/src/ops/function.rs:227
note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.
```

Only if we fix both, can we actually add it to our `HeaderMap`:

```rust
use hyper::{header::HeaderValue, HeaderMap};

fn main() {
    let mut headers = HeaderMap::new();
    headers.insert("Born", HeaderValue::from_static("elegant"));
    dbg!(headers);
}
```

```shell
$ cargo run --quiet
[src/main.rs:6] headers = {
    "born": "elegant",
}
```

Note also that `HeaderMap` normalizes header names - since the RFC says that
header names are case insensitive.

What if we don't want to panic? Say, if our header names and values come from
user input?

We can just use the non-panicking variants!

Let's give it a shot:

```rust
use hyper::{
    header::{HeaderName, HeaderValue},
    HeaderMap,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut headers = HeaderMap::new();
    while let (Some(k), Some(v)) = (args.next(), args.next()) {
        if let Ok(k) = HeaderName::from_bytes(k.as_bytes()) {
            if let Ok(v) = HeaderValue::from_bytes(v.as_bytes()) {
                headers.insert(k, v);
            } else {
                println!("Skipping invalid header value {}", v);
            }
        } else {
            println!("Skipping invalid header name {}", k);
        }
    }
    dbg!(headers);
}
```

```shell
$ cargo run --quiet -- host example.org née élégante born élégante born elegant
Skipping invalid header name née
[src/main.rs:20] headers = {
    "host": "example.org",
    "born": "elegant",
}
```

{% sc bearsays %}
Super neat! But wait... where's the message that says "Skipping invalid
header value élégante"?
{% endsc %}

A fair question - since that message wasn't printed it's safe to assume that
"élégante" is _not_ in fact, an invalid header value. Let's check the
documentation of `HeaderValue` to see what's up:

```
/// Represents an HTTP header field value.
///
/// In practice, HTTP header field values are usually valid ASCII. However, the
/// HTTP spec allows for a header value to contain opaque bytes as well. In this
/// case, the header field value is not able to be represented as a string.
///
/// To handle this, the `HeaderValue` is useable as a type and can be compared
/// with strings and implements `Debug`. A `to_str` fn is provided that returns
/// an `Err` if the header value contains non visible ascii characters.
```

AhAH! So HTTP _does_ allow non-ASCII headers, but they're not "strings", so
`HeaderValue::from_static` disallows them.

However, if we switch from `HeaderMap::insert` to `HeaderMap::append`, we can
see that both our `born` headers were accepted:

```rust
    while let (Some(k), Some(v)) = (args.next(), args.next()) {
        if let Ok(k) = HeaderName::from_bytes(k.as_bytes()) {
            if let Ok(v) = HeaderValue::from_bytes(v.as_bytes()) {
                // NEW! (was headers.insert)
                headers.append(k, v);
            } else {
                println!("Skipping invalid header value {}", v);
            }
        } else {
            println!("Skipping invalid header name {}", k);
        }
    }
```

```shell
$ cargo run --quiet -- host example.org née élégante born élégante born elegant
Skipping invalid header name née
[src/main.rs:21] headers = {
    "host": "example.org",
    "born": "\xc3\xa9l\xc3\xa9gante",
    "born": "elegant",
}
```

Now, I don't know about you, but I'm impressed. I didn't even _know_ that
hyper did that. But when you have a language that lets you model a problem
properly, it's not exactly a surprise when people do.

And that's an important point as well - you _could_ have a Rust HTTP
implementation that just uses `HashMap<String, Vec<String>>` - but why do
that when you can have a high-performance multimap, which is fast in the 90%
case and still correct the rest of the time?

{% sc tip %}
`hyper` even goes so far as to have enum values for common headers, so
there's no allocation required to store the name of headers like
"accept-charset", "host", or "www-authenticate".
{% endsc %}

And you _could_ have a Go HTTP library that has a slightly better structure
than the official one... and in fact [people have done exactly
that](https://github.com/valyala/fasthttp) - but then you lose out on a
_huge_ part of the ecosystem because this is not a thing Go encourages. At
all.

In Go, we just want most things to work out most of the time. And if they really
don't, well... we can probably just patch it. And if we can't, well, we're in
deep trouble but we could always just write [a code generator](https://github.com/kubernetes/code-generator).

## Enough with the comparisons already

As I've mentioned before, a lot of discussions around programming languages
quickly becomes heated - it's as if we're cheering for sports teams instead
of discussing systems.

I'm wholly uninterested in cheering for a team. I am _very_ interested in
systems that prevent mistakes, or even better, _entire classes_ of mistakes.

When you hear someone talk about how much they love Rust, once they've really
started loving it, it's hard to take it at face value - especially if you've
already practiced different programming languages before.

If you've been following industry trends (because, well, of the job market),
you've probably experienced Ruby, Python, JavaScript, Java, Go, C, etc.

And while there are significant differences between these languages, in terms
of how effective they are at letting you model a problem "correctly"... it's
not night and day.

You might have to write a lot more assertions in C, boilerplate in Java, and
write a lot more tests in dynamic languages, but they're more or less all
equally permissive in terms of letting you "construct impossible values",
which _cannot_ be processed meaningfully and end up polluting your whole
codebase with unending validation - if you care enough to do it, anyway.

In terms of modelling a problem, Rust really is _several steps_ above those
languages. But it's not alien technology - it's not completely removed from
existing systems, in an ivory tower. It exists as a compromise, that
significantly improves the status quo _and_ integrates well.

This is what makes Rust unique to me. Of course Rust was strongly influenced
by languages that came before it. Again: the value is in the compromise.

Memory management is a particularly big hurdle for folks moving from the
languages I mentioned - I've argued before that it's not manual memory
management, it's more [declarative memory
management](/articles/declarative-memory-management).

But much like most of what Rust provides, you can opt into it over time.

It's fine to prototype something with `String` and `clone` whenever you need
to. Or use an
[Arc](https://doc.rust-lang.org/stable/std/sync/struct.Arc.html). And later
you can figure out if it's worth replacing with some borrowed types, for
performance. You don't have to come up with the most performance design
upfront (even though it's real tempting!).

Over time, though, if you commit to writing Rust and trying to really go all
the way into what it _encourages_ you to do (write safe, correct code),
you'll find yourself thinking differently: writing types and function
signatures first, implementations later.

But also, restructuring your program so that state is neatly separated, so
you don't get into heated discussion with the borrow checker. Fields will
start being grouped by "mutation affinity" rather than by "theme", as you may
have done in other languages previously. You'll end up naming quite a few
structs `State`.

It really is a wonderful journey, and even if you still have to write other
languages for your day job, the experience you'll acquire learning Rust is
applicable in other languages too - even C++!

Hopefully this article doesn't just add to the pile - it's hard to advocate
for a solution without pointing how other solutions fail to address specific
problems, so a bit of comparison was unavoidable.

If you want to learn Rust, there are _many_ excellent resources online,
like the official [Rust book](https://doc.rust-lang.org/stable/book/).

If you enjoyed my writing, there's [a lot more of it](/tags/rust) specific to
Rust. I even have [entire series](/series).

No matter your path to Rust, I guarantee you'll at least learn _something_
that is applicable to your trade elsewhere. And if you don't, well, you
can always [contribute to it](https://github.com/rust-lang/rust)!
