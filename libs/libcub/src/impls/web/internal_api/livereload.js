let socket;
let wasDisconnected = false;

function connectWebSocket() {
    socket = new WebSocket(`ws://${window.location.host}/internal-api/ws`);

    socket.onmessage = function (event) {
        let data = JSON.parse(event.data);
        if (data.new_revision) {
            location.reload();
        }
    };

    socket.onerror = function (error) {
        console.error("WebSocket Error:", error);
    };

    socket.onclose = function (event) {
        console.log("WebSocket connection closed:", event.code, event.reason);
        wasDisconnected = true;
        setTimeout(connectWebSocket, 5000); // Try to reconnect after 5 seconds
    };

    socket.onopen = function () {
        console.log("WebSocket connected.");
        if (wasDisconnected) {
            console.log("Reconnected after disconnection. Reloading page.");
            location.reload();
        }
        wasDisconnected = false;
    };
}

connectWebSocket();

document.addEventListener("click", function (event) {
    if (event.target.tagName === "A" && event.target.href.startsWith("home://")) {
        event.preventDefault();

        let filePath = event.target.href.substring(7); // Remove 'home://'

        fetch("/internal-api/open-in-editor", {
            method: "POST",
            headers: {
                "Content-Type": "application/json",
            },
            body: JSON.stringify({ input_path: filePath }),
        })
            .then((response) => {
                if (!response.ok) {
                    throw new Error("Network response was not ok");
                }
                console.log("File opened in editor");
            })
            .catch((error) => {
                console.error("Error opening file in editor:", error);
            });
    }
});
