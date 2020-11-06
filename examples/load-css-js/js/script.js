function buttonPressed() {
    document.getElementById("button-status").innerHTML = "Button Clicked!"
    setTimeout(function () {
        document.getElementById("button-status").innerHTML = ""
    }, 2000);
}